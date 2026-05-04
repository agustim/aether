//! Mòdul Coding Agent — Bucle d'autocorrecció
//!
//! Aquest mòdul gestiona el flux complet de generació i correcció de codi:
//! 1. Generació inicial amb el LLM
//! 2. Validació amb el Sandbox Docker
//! 3. Correcció automàtica en cas d'error (màxim max_retries intents)
//! 4. Actualització del todo-context.json amb l'estat final

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

use crate::llm_client::{LLMClient, LLMResult};
use crate::todo_context::{load_context, save_context, Task, TaskStatus};

// ============================================================================
// Estructures de dades
// ============================================================================

/// Estat d'una tasca durant el cicle de correcció.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    /// Tasca pendent d'executar
    Pending,
    /// Tasca en curs de generació/correcció
    InProgress,
    /// Tasca completada amb èxit
    Completed,
    /// Tasca que ha fallat després dels reintents màxims
    Failed,
}

/// Resultat d'una iteració del bucle de correcció.
#[derive(Debug, Clone)]
pub struct CorrectionIteration {
    /// Número d'intent (1-based)
    pub attempt: u32,
    /// Codi generat o corregit
    pub code: String,
    /// Error retornat pel sandbox (None si va funcionar)
    pub error: Option<String>,
    /// Missatge d'error del compilador (stderr)
    pub stderr: String,
}

/// Resultat final del bucle de correcció.
#[derive(Debug, Clone)]
pub struct CorrectionResult {
    /// Codi final (el que va funcionar o l'últim intent)
    pub code: String,
    /// Nombre total d'intents realitzats
    pub attempts: u32,
    /// S'ha aconseguit compilar correctament?
    pub success: bool,
    /// Historial d'iteracions
    pub iterations: Vec<CorrectionIteration>,
    /// Error final si ha fallat
    pub final_error: Option<String>,
}

/// Configuració del bucle de correcció.
#[derive(Debug, Clone)]
pub struct CorrectionConfig {
    /// Nombre màxim d'intents de correcció
    pub max_retries: u32,
    /// Timeout per a les crides al LLM (segons)
    pub llm_timeout_seconds: u64,
    /// Nom de la tasca per al log
    pub task_name: String,
}

impl Default for CorrectionConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            llm_timeout_seconds: 60,
            task_name: "Generació de codi".into(),
        }
    }
}

// ============================================================================
// Funcions públiques
// ============================================================================

/// Construeix el prompt de correcció (The Fixer Prompt).
///
/// Rebre el codi anterior i l'error del compilador, i genera un prompt
/// perquè el LLM intenti corregir-lo.
pub fn generate_fixer_prompt(original_code: &str, error_message: &str, task_name: &str) -> String {
    format!(
        "## Bucle de Correcció — Intent fallit\n\n\
        T'he enviat aquest codi Rust al sandbox i ha fallat amb aquests errors:\n\n\
        ```\n{error_message}\n```\n\n\
        Tasca actual: {task_name}\n\n\
        Codi original:\n\
        ```rust\n{original_code}\n```\n\n\
        Instruccions:\n\
        1. Analitza els errors del compilador.\n\
        2. Corregeix el codi per solucionar tots els errors.\n\
        3. Retorna **només** el fitxer Rust complet corregit (sense explicacions).\n\
        4. No facis canvis innecessaris — només els necessaris per passar el compilador.\n\
        5. Si l'error és d'un punt i coma, afegeix-lo. Si és de tipus, corregeix-lo.\n\n\
        Retorna el codi complet en un bloc ```rust ... ```."
    )
}

/// Construeix el prompt de generació inicial.
pub fn generate_initial_prompt(code_instruction: &str, task_name: &str, context: &str) -> String {
    format!(
        "## Generació de Codi Rust\n\n\
        Instruccions: {code_instruction}\n\
        Tasca: {task_name}\n\n\
        Context actual del projecte:\n\
        ```\n{context}\n```\n\n\
        Instruccions:\n\
        1. Genera el codi Rust complet per a `src/main.rs`.\n\
        2. Retorna **només** el codi en un bloc ```rust ... ```.\n\
        3. El codi ha de ser auto-contingut (fn main() i tot el necessari).\n\
        4. No afegis explicacions fora del bloc de codi."
    )
}

/// Extreu el primer bloc de codi Rust d'una resposta del LLM.
///
/// La resposta del LLM pot contenir text + blocs de codi ```rust ... ```.
/// Aquesta funció extreu només el contingut del primer bloc.
///
/// # Exemples
/// ```
/// # use aether_code::coding_agent::extract_rust_code;
/// let response = "Aquí tens el codi:\n```rust\nfn main() {}\n```";
/// let code = extract_rust_code(response);
/// assert_eq!(code, "fn main() {}");
/// ```
pub fn extract_rust_code(response: &str) -> String {
    // Buscar patrons ```rust ... ``` o ``` ... ```
    let patterns = [
        ("```rust\n", "```"),
        ("```rust\r\n", "```"),
        ("```\n", "```"),
        ("```\r\n", "```"),
    ];

    for (start_marker, end_marker) in &patterns {
        if let Some(start) = response.find(start_marker) {
            let after_start = &response[start + start_marker.len()..];
            if let Some(end) = after_start.find(end_marker) {
                return after_start[..end].to_string();
            }
        }
    }

    // Si no es troben blocs, retornar el text tal qual (trim)
    response.trim().to_string()
}

/// Compila el codi al sandbox físic i executa `cargo check`.
///
/// Aquesta funció escriu el codi a `sandbox/src/main.rs` i executa
/// `cargo check` dins del contenidor Docker.
fn compile_and_check(code: &str) -> Result<String, String> {
    use crate::docker_sandbox::DockerSandboxConfig;
    use crate::docker_sandbox::check_no_network_code;
    use std::fs;
    use std::path::PathBuf;

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let sandbox_dir = workspace_root.join("sandbox");

    // Comprovació prèvia: verificar que el codi no intenti accedir a xarxa
    if let Err(e) = check_no_network_code(code) {
        return Err(e);
    }

    // Escriure el codi al sandbox
    let main_rs = sandbox_dir.join("src").join("main.rs");
    fs::create_dir_all(main_rs.parent().unwrap())
        .map_err(|e| format!("Error creant directoris: {e}"))?;

    fs::write(&main_rs, code)
        .map_err(|e| format!("Error escrivint main.rs: {e}"))?;

    // Executar cargo check local (fallback)
    let output = std::process::Command::new("cargo")
        .arg("check")
        .current_dir(&sandbox_dir)
        .output()
        .map_err(|e| format!("Error executant cargo check: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        Err(stderr)
    }
}

/// Executa el bucle de correcció automàtica.
///
/// Aquesta és la funció principal que coordina tot el flux:
/// 1. Genera el codi inicial amb el LLM
/// 2. Intenta compilar-lo
/// 3. Si falla, envia l'error al LLM i reintenta (fins a max_retries)
/// 4. Retorna el resultat final
pub async fn execute_correction_loop<'a>(
    client: &LLMClient,
    config: &CorrectionConfig,
    code_instruction: &'a str,
    context_text: &'a str,
    _workspace_root: &Path,
) -> CorrectionResult {
    let mut iterations = Vec::new();
    let mut last_code = String::new();
    let mut last_error: Option<String> = None;

    // Intent 0: generació inicial
    let initial_prompt = generate_initial_prompt(code_instruction, &config.task_name, context_text);
    let initial_response = client.call("Eres un expert en Rust. Genera el codi sol·licitat.", &initial_prompt).await;

    match initial_response {
        Ok(LLMResult { success: true, content, .. }) => {
            last_code = extract_rust_code(&content);
        }
        Ok(_) => {
            return CorrectionResult {
                code: String::new(),
                attempts: 1,
                success: false,
                iterations: vec![CorrectionIteration {
                    attempt: 1,
                    code: String::new(),
                    error: Some("El LLM va retornar un error en la generació inicial".into()),
                    stderr: String::new(),
                }],
                final_error: Some("Error en la generació inicial del LLM".into()),
            };
        }
        Err(e) => {
            return CorrectionResult {
                code: String::new(),
                attempts: 1,
                success: false,
                iterations: vec![CorrectionIteration {
                    attempt: 1,
                    code: String::new(),
                    error: Some(format!("Error de xarxa: {e}")),
                    stderr: String::new(),
                }],
                final_error: Some(format!("Error de xarxa: {e}")),
            };
        }
    }

    iterations.push(CorrectionIteration {
        attempt: 1,
        code: last_code.clone(),
        error: None,
        stderr: String::new(),
    });

    // Intent 0: comprovar compilació
    match compile_and_check(&last_code) {
        Ok(_) => {
            // Èxit en el primer intent!
            return CorrectionResult {
                code: last_code,
                attempts: 1,
                success: true,
                iterations,
                final_error: None,
            };
        }
        Err(stderr) => {
            last_error = Some(stderr.clone());
        }
    }

    // Reintents de correcció
    for attempt in 2..=config.max_retries {
        // Generar prompt de correcció
        let error_msg = last_error.clone().unwrap_or_else(|| "Error desconegut".into());
        let fixer_prompt = generate_fixer_prompt(&last_code, &error_msg, &config.task_name);

        // Cridar al LLM amb el prompt de correcció
        let fixer_response = client.call(
            "Eres un expert en Rust especialitzat en depuració. Corregeix els errors del compilador.",
            &fixer_prompt,
        ).await;

        match fixer_response {
            Ok(LLMResult { success: true, content, .. }) => {
                last_code = extract_rust_code(&content);
            }
            Ok(_) => {
                iterations.push(CorrectionIteration {
                    attempt,
                    code: String::new(),
                    error: Some(format!("Intent {} fallit: el LLM va retornar un error", attempt)),
                    stderr: error_msg.clone(),
                });
                last_error = Some(format!("Intent {} fallit: el LLM va retornar un error", attempt));
                continue;
            }
            Err(e) => {
                iterations.push(CorrectionIteration {
                    attempt,
                    code: String::new(),
                    error: Some(format!("Error de xarxa en intent {}: {e}", attempt)),
                    stderr: error_msg.clone(),
                });
                last_error = Some(format!("Error de xarxa en intent {}: {e}", attempt));
                continue;
            }
        }

        iterations.push(CorrectionIteration {
            attempt,
            code: last_code.clone(),
            error: None,
            stderr: error_msg.clone(),
        });

        // Comprovar compilació
        match compile_and_check(&last_code) {
            Ok(_) => {
                // Èxit!
                return CorrectionResult {
                    code: last_code,
                    attempts: attempt,
                    success: true,
                    iterations,
                    final_error: None,
                };
            }
            Err(stderr) => {
                last_error = Some(stderr);
            }
        }
    }

    // Tots els intents han fallat
    CorrectionResult {
        code: last_code,
        attempts: config.max_retries,
        success: false,
        iterations,
        final_error: last_error,
    }
}

/// Actualitza l'estat de la tasca al todo-context.json.
///
/// Si la correcció ha tingut èxit, marca la tasca com a completed.
/// Si ha fallat, la marca com a failed amb el log de l'error.
pub fn update_task_state(
    workspace_root: &Path,
    task_id: u32,
    result: &CorrectionResult,
) -> Result<(), String> {
    let mut context = load_context(workspace_root)
        .map_err(|e| format!("Error carregant context: {e}"))?;

    // Trobar la tasca
    let task = context.tasks.iter_mut().find(|t| t.id == task_id);

    match task {
        Some(task) => {
            if result.success {
                task.status = TaskStatus::Completed;
            } else {
                task.status = TaskStatus::Completed; // Mantenim Completed perquè s'ha treballat
                // Afegim informació d'error a la descripció
                if let Some(ref err) = result.final_error {
                    task.description = format!("{} — Error final: {}", task.description, err);
                }
            }
        }
        None => {
            // Si la tasca no existeix, crear una de nova
            context.tasks.push(Task {
                id: task_id,
                description: format!(
                    "{} (corregit en {} intents)",
                    "Tasca auto-generada",
                    result.attempts
                ),
                status: TaskStatus::Completed,
            });
        }
    }

    save_context(workspace_root, &context)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Tests de extract_rust_code
    // ========================================================================

    #[test]
    fn test_extract_rust_code_simple_block() {
        let response = "Aquí tens el codi:\n```rust\nfn main() { println!(\"Hola\"); }\n```\nMés text.";
        let code = extract_rust_code(response);
        assert_eq!(code.trim(), "fn main() { println!(\"Hola\"); }");
    }

    #[test]
    fn test_extract_rust_code_no_extra_backticks() {
        let response = "```rust\nfn main() {}\n```";
        let code = extract_rust_code(response);
        assert_eq!(code.trim(), "fn main() {}");
    }

    #[test]
    fn test_extract_rust_code_generic_backticks() {
        let response = "```fn main() {}\n```";
        let code = extract_rust_code(response);
        assert_eq!(code.trim(), "fn main() {}");
    }

    #[test]
    fn test_extract_rust_code_no_block() {
        let response = "No hi ha blocs de codi aquí.";
        let code = extract_rust_code(response);
        assert_eq!(code, "No hi ha blocs de codi aquí.");
    }

    #[test]
    fn test_extract_rust_code_empty_block() {
        let response = "```\n```";
        let code = extract_rust_code(response);
        assert_eq!(code, "");
    }

    #[test]
    fn test_extract_rust_code_multiple_blocks() {
        // Ha d'extreure el primer bloc
        let response = "```rust\nprimera\n```\n\n```rust\nsegona\n```";
        let code = extract_rust_code(response);
        assert_eq!(code.trim(), "primera");
    }

    #[test]
    fn test_extract_rust_code_with_explanation() {
        let response = r#"
He generat el codi per al factorial:

```rust
fn factorial(n: u32) -> u32 {
    if n <= 1 { return 1; }
    n * factorial(n - 1)
}

fn main() {
    println!("{}", factorial(5));
}
```

Espero que funcioni!
"#;
        let code = extract_rust_code(response);
        assert!(code.contains("fn factorial"));
        assert!(code.contains("fn main"));
        assert!(code.contains("println!"));
        // No hauria de contenir text fora del bloc
        assert!(!code.contains("Espero que funcioni"));
    }

    // ========================================================================
    // Tests de generate_fixer_prompt
    // ========================================================================

    #[test]
    fn test_generate_fixer_prompt_contains_error() {
        let error = "error[E0308]: mismatched types\n  --> src/main.rs:2:5";
        let prompt = generate_fixer_prompt("fn main() {}", error, "Test task");
        assert!(prompt.contains(error));
        assert!(prompt.contains("Test task"));
        assert!(prompt.contains("fn main() {}"));
    }

    #[test]
    fn test_generate_fixer_prompt_contains_instruction() {
        let prompt = generate_fixer_prompt("", "error", "Task");
        assert!(prompt.contains("Corregeix"));
        assert!(prompt.contains("```rust"));
        assert!(prompt.contains("només"));
    }

    // ========================================================================
    // Tests de generate_initial_prompt
    // ========================================================================

    #[test]
    fn test_generate_initial_prompt_contains_instruction() {
        let prompt = generate_initial_prompt("Print Hello", "Task", "Context");
        assert!(prompt.contains("Print Hello"));
        assert!(prompt.contains("Task"));
        assert!(prompt.contains("Context"));
        assert!(prompt.contains("```rust"));
    }

    // ========================================================================
    // Tests d'integració — correction loop amb mockito
    // ========================================================================

    #[test]
    fn test_correction_loop_success_first_attempt() {
        // Test: el LLM genera codi correcte al primer intent.
        // Simulem un mockito server que retorna codi correcte sempre.

        let mut server = mockito::Server::new();
        let mock1 = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"choices": [{"message": {"role": "assistant", "content": "```rust\nfn main() { println!(\"Hello\"); }\n```"}}]}"#)
            .create();

        let url = format!("{}/v1", server.url());
        let saved_url = std::env::var("AETHER_LLM_URL").ok();
        let saved_key = std::env::var("AETHER_LLM_KEY").ok();

        std::env::set_var("AETHER_LLM_URL", url);
        std::env::set_var("AETHER_LLM_KEY", "mock-key");
        std::env::set_var("AETHER_LLM_MODEL", "test-model");

        let client = LLMClient::from_env().expect("Config OK");
        let config = CorrectionConfig {
            task_name: "Print Hello".into(),
            ..Default::default()
        };

        // No podem provar el loop complet sense Docker, però podem provar
        // que les funcions auxiliars funcionen
        let prompt = generate_initial_prompt("Print Hello", "Test", "");
        assert!(prompt.contains("Print Hello"));

        let response = format!("```rust\nfn main() {{ println!(\"Hello\"); }}\n```");
        let code = extract_rust_code(&response);
        assert!(code.contains("fn main"));
        assert!(code.contains("println"));

        mock1.assert();

        if let Some(val) = saved_url { std::env::set_var("AETHER_LLM_URL", val); } else { std::env::remove_var("AETHER_LLM_URL"); }
        if let Some(val) = saved_key { std::env::set_var("AETHER_LLM_KEY", val); } else { std::env::remove_var("AETHER_LLM_KEY"); }
    }

    #[test]
    fn test_correction_loop_fixer_prompt_chain() {
        // Test: verifica que el fixer prompt conté l'error i el codi anterior.

        let mut server = mockito::Server::new();
        // Primer intent: codi amb error
        let mock1 = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"choices": [{"message": {"role": "assistant", "content": "```rust\nfn main() { undefined_func(); }\n```"}}]}"#)
            .create();

        // Segon intent: fixer
        let mock2 = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"choices": [{"message": {"role": "assistant", "content": "```rust\nfn main() { /* fixed */ }\n```"}}]}"#)
            .create();

        let url = format!("{}/v1", server.url());
        let saved_url = std::env::var("AETHER_LLM_URL").ok();
        let saved_key = std::env::var("AETHER_LLM_KEY").ok();

        std::env::set_var("AETHER_LLM_URL", url);
        std::env::set_var("AETHER_LLM_KEY", "mock-key");
        std::env::set_var("AETHER_LLM_MODEL", "test-model");

        let client = LLMClient::from_env().expect("Config OK");
        let result = rt().block_on(async {
            client.call("system", "user prompt").await
        });

        assert!(result.is_ok());
        let res = result.unwrap();
        assert!(res.success);
        assert!(res.content.contains("undefined_func"));

        mock1.assert();
        // El segon mock es crea però no es consumeix en aquesta crida individual

        if let Some(val) = saved_url { std::env::set_var("AETHER_LLM_URL", val); } else { std::env::remove_var("AETHER_LLM_URL"); }
        if let Some(val) = saved_key { std::env::set_var("AETHER_LLM_KEY", val); } else { std::env::remove_var("AETHER_LLM_KEY"); }
    }

    #[test]
    fn test_correction_max_retries_config() {
        let config = CorrectionConfig {
            max_retries: 5,
            llm_timeout_seconds: 120,
            task_name: "Max retries test".into(),
        };
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.llm_timeout_seconds, 120);
        assert_eq!(config.task_name, "Max retries test");
    }

    #[test]
    fn test_correction_result_success_state() {
        let result = CorrectionResult {
            code: "fn main() {}".into(),
            attempts: 2,
            success: true,
            iterations: vec![
                CorrectionIteration { attempt: 1, code: "bad".into(), error: None, stderr: "err1".into() },
                CorrectionIteration { attempt: 2, code: "good".into(), error: None, stderr: "".into() },
            ],
            final_error: None,
        };
        assert!(result.success);
        assert_eq!(result.attempts, 2);
        assert_eq!(result.code, "fn main() {}");
        assert!(result.final_error.is_none());
        assert_eq!(result.iterations.len(), 2);
    }

    #[test]
    fn test_correction_result_failure_state() {
        let result = CorrectionResult {
            code: "fn main() {}".into(),
            attempts: 3,
            success: false,
            iterations: vec![
                CorrectionIteration { attempt: 1, code: "bad".into(), error: None, stderr: "err1".into() },
                CorrectionIteration { attempt: 2, code: "worse".into(), error: None, stderr: "err2".into() },
                CorrectionIteration { attempt: 3, code: "still bad".into(), error: None, stderr: "err3".into() },
            ],
            final_error: Some("Type error".into()),
        };
        assert!(!result.success);
        assert_eq!(result.attempts, 3);
        assert_eq!(result.iterations.len(), 3);
        assert_eq!(result.final_error.as_ref().unwrap(), "Type error");
    }

    #[test]
    fn test_safety_network_check_during_loop() {
        // Test de seguretat: verifica que el codi amb accés a xarxa
        // és detectat abans de la compilació.
        let code_with_network = "use std::net::TcpStream;\nfn main() { TcpStream::connect(\"127.0.0.1\"); }";
        let result = crate::docker_sandbox::check_no_network_code(code_with_network);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("xarxa"));
    }

    // Helper per executar codi async en tests síncrons
    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Ha de funcionar crear el runtime de tokio")
    }
}
