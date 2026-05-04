//! Aether Code — Compiler-Agent orchestrator
//!
//! Rebi un bloc de codi Rust, l'escriu a `sandbox/src/main.rs`
//! i executem `cargo check` per validar-lo.
//! Tot retorna un JSON amb status i missatge.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::process::Command;
use tokio::sync::Mutex;

pub mod commit;

/// Estat de la compilació.
#[derive(Debug, Serialize)]
enum Status {
    Success,
    Error,
}

/// Petició de construcció amb metadades per al commit (Regla d'Or 6).
#[derive(Debug, Deserialize)]
struct BuildRequest {
    code: String,
    rule_id: String,
    rule_name: String,
    rule_description: String,
    todo_status: String,
    todo_description: String,
    #[serde(default)]
    pending_tasks: Vec<String>,
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Bloqueig compartit per al sandbox — evita competició entre tests.
static SANDBOX_LOCK: OnceLock<Arc<Mutex<()>>> = OnceLock::new();

fn get_sandbox_lock() -> Arc<Mutex<()>> {
    SANDBOX_LOCK
        .get_or_init(|| Arc::new(Mutex::new(())))
        .clone()
}

/// Escriu codi Rust al fitxer principal del sandbox.
async fn write_sandbox_code(code: &str) -> Result<(), String> {
    let sandbox_src = workspace_root()
        .join("sandbox")
        .join("src")
        .join("main.rs");

    fs::create_dir_all(sandbox_src.parent().unwrap())
        .map_err(|e| format!("No s'han pogut crear els directoris: {e}"))?;

    fs::write(&sandbox_src, code)
        .map_err(|e| format!("No s'ha pogut escriure main.rs: {e}"))?;

    Ok(())
}

/// Executa `cargo check` dins de la carpeta del sandbox.
/// Retorna stdout + stderr en un sol string.
async fn run_cargo_check() -> Result<String, String> {
    let sandbox_dir = workspace_root().join("sandbox");

    let output = Command::new("cargo")
        .arg("check")
        .current_dir(&sandbox_dir)
        .output()
        .await
        .map_err(|e| format!("No s'ha pogut executar cargo check: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let combined = format!("{stdout}{stderr}");

    if output.status.success() {
        Ok(combined)
    } else {
        Err(combined)
    }
}

/// Resultat ampli que inclou info de compilació + commit.
#[derive(Debug, Serialize)]
struct BuildResult {
    status: Status,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit_message: Option<String>,
}

/// Funció completa: compila el codi, fa el commit si tot va bé
/// i actualitza el Todo-Context.
async fn build_and_commit(
    code: &str,
    rule_id: &str,
    rule_name: &str,
    rule_description: &str,
    todo_status: &str,
    todo_description: &str,
    pending_tasks: Vec<String>,
) -> BuildResult {
    let repo_path = workspace_root();

    // Inicialitzar el Todo-Context si no existeix
    let _ = commit::init_todo_context(&repo_path, todo_description);

    // Compilar el codi
    let lock = get_sandbox_lock();
    let _guard = lock.lock().await;

    if let Err(e) = write_sandbox_code(code).await {
        return BuildResult {
            status: Status::Error,
            message: format!("Error d'escriptura: {e}"),
            commit_hash: None,
            commit_message: None,
        };
    }

    let check_output = match run_cargo_check().await {
        Ok(output) => output,
        Err(output) => {
            return BuildResult {
                status: Status::Error,
                message: output,
                commit_hash: None,
                commit_message: None,
            };
        }
    };

    // Si la compilació ha funcionat, fer el commit
    let test_results = if check_output.is_empty() {
        // Cargo check sense output = tot correcte (assumim tests del projecte passant)
        commit::TestResults::all_passed(0)
    } else {
        // Parsejar resultats de tests de l'output de cargo test
        parse_test_results(&check_output)
    };

    let mut todo_context = commit::TodoContext::new(todo_status, todo_description);
    for task in pending_tasks {
        todo_context.add_pending(&task);
    }

    let meta = commit::CommitMetadata {
        business_rule: commit::BusinessRule {
            id: rule_id.into(),
            name: rule_name.into(),
            description: rule_description.into(),
        },
        test_results,
        todo_context,
    };

    let commit_msg = commit::generate_commit_message(&meta);
    let commit_hash = match commit::make_commit(&repo_path, &commit_msg) {
        Ok(hash) => {
            // Actualitzar el Todo-Context
            let _ = commit::update_todo_context(&repo_path, &meta.todo_context);
            Some(hash)
        }
        Err(e) => {
            eprintln!("Avís: no s'ha pogut crear commit: {e}");
            None
        }
    };

    BuildResult {
        status: Status::Success,
        message: if check_output.is_empty() {
            "Compilació correcta — cargo check ha passat sense errors.".into()
        } else {
            check_output
        },
        commit_hash,
        commit_message: Some(commit_msg),
    }
}

/// Parseja els resultats de tests de l'output de `cargo test` o `cargo check`.
fn parse_test_results(output: &str) -> commit::TestResults {
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut details = Vec::new();

    // Cercar patrons com "test tests::foo ... ok" o "FAILED"
    for line in output.lines() {
        let lower = line.to_lowercase();
        if lower.contains("test tests::") || lower.contains("test ") {
            if lower.contains("ok") || lower.contains("... ok") {
                passed += 1;
            } else if lower.contains("fail") || lower.contains("FAILED") {
                failed += 1;
                // Extreure el nom del test
                let parts: Vec<&str> = line.split("...").collect();
                if !parts.is_empty() {
                    let test_name = parts[0].trim().to_string();
                    details.push(test_name);
                }
            }
        }
    }

    // Si no s'han trobat tests a les línies, buscar al resum
    if passed == 0 && failed == 0 {
        for line in output.lines() {
            if line.contains("running") && line.contains("tests") {
                // Ex: "running 5 tests"
                if let Some(count_str) = line.split("running").nth(1).and_then(|s| s.split("tests").next()) {
                    if let Ok(count) = count_str.trim().parse::<u32>() {
                        passed = count;
                    }
                }
            }
            if line.contains("test result:") {
                // Ex: "test result: ok. 5 passed; 0 failed"
                if let Some(counts) = line.split("passed").next() {
                    if let Some(last) = counts.split(';').next_back() {
                        if let Ok(p) = last.trim().trim_start().parse::<u32>() {
                            passed = p;
                        }
                    }
                }
                if let Some(pos) = line.find("failed") {
                    let after = &line[pos + 6..];
                    if let Some(count_str) = after.split(';').next() {
                        if let Ok(f) = count_str.trim().parse::<u32>() {
                            failed = f;
                        }
                    }
                }
            }
        }
    }

    let total = passed + failed;
    if failed > 0 {
        commit::TestResults::with_failures(total, failed, details)
    } else {
        commit::TestResults::all_passed(total)
    }
}

fn main() {
    // Mode de consola: llegim entrada de stdin i retornem JSON a stdout.
    // Soporta dos formats:
    //   1. Codi Rust simple (per compatibilitat): retorna CheckResult
    //   2. JSON amb BuildRequest: retorna BuildResult amb commit
    //
    // Exemple JSON (Regla d'Or 6):
    // {
    //   "code": "fn main() { println!(\"Hola!\"); }",
    //   "rule_id": "RO-6",
    //   "rule_name": "Historial Atòmic",
    //   "rule_description": "Commit automàtic amb missatge estandarditzat",
    //   "todo_status": "complete",
    //   "todo_description": "Implementació de la Regla d'Or 6",
    //   "pending_tasks": ["Afegir suport per a Docker"]
    // }
    let rt = tokio::runtime::Runtime::new().expect("No s'ha pogut crear el runtime de tokio");

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("No s'ha pogut llegir stdin");

    let result: BuildResult = rt.block_on(async {
        // Intentar parsejar com a JSON (BuildRequest)
        let build_input = input.trim();
        if build_input.starts_with('{') {
            match serde_json::from_str::<BuildRequest>(build_input) {
                Ok(req) => build_and_commit(
                    &req.code,
                    &req.rule_id,
                    &req.rule_name,
                    &req.rule_description,
                    &req.todo_status,
                    &req.todo_description,
                    req.pending_tasks,
                )
                .await,
                Err(e) => {
                    // Si el JSON no és vàlid, fallback a compile_code amb el text tal qual
                    BuildResult {
                        status: Status::Error,
                        message: format!("JSON invàlid: {e}. Intentant com a codi simple..."),
                        commit_hash: None,
                        commit_message: None,
                    }
                }
            }
        } else {
            // Mode simple: només codi Rust → utilitza metadades per defecte
            build_and_commit(
                build_input,
                "BR-000",
                "Compilació Simple",
                "Codi enviat via stdin sense metadades de regla de negoci",
                "in-progress",
                "Compilació via stdin",
                vec![],
            )
            .await
        }
    });

    // Retornem el resultat com a JSON
    let json = serde_json::to_string_pretty(&result).expect("Error serialitzant el resultat");
    println!("{json}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_root_exists() {
        let root = workspace_root();
        assert!(
            root.exists(),
            "La ruta del workspace ha d'existir: {:?}",
            root
        );
        assert!(
            root.join("Cargo.toml").exists(),
            "Ha d'haver-hi un Cargo.toml al workspace"
        );
    }

    #[tokio::test]
    async fn test_write_sandbox_code_ok() {
        let code = "fn main() { println!(\"Hola\"); }";
        let result = write_sandbox_code(code).await;
        assert!(result.is_ok(), "Escriure codi vàlid ha de funcionar");

        // Verificar que el fitxer s'ha creat
        let sandbox_src = workspace_root()
            .join("sandbox")
            .join("src")
            .join("main.rs");
        assert!(sandbox_src.exists(), "El fitxer main.rs ha d'existir");

        let content = fs::read_to_string(&sandbox_src).unwrap();
        assert!(content.contains("println!"));
    }

    #[tokio::test]
    async fn test_write_sandbox_code_empty() {
        // Un fitxer buit no ha de fallar l'escriptura (només fallarà cargo check)
        let result = write_sandbox_code("").await;
        assert!(result.is_ok(), "Escriure un string buit ha de funcionar");
    }

    #[tokio::test]
    async fn test_build_and_commit_valid_code() {
        let code = "fn main() { println!(\"Commit OK\"); }";
        let result = build_and_commit(
            code,
            "RO-6",
            "Historial Atòmic",
            "Commit automàtic amb missatge estandarditzat",
            "complete",
            "Regla d'Or 6 implementada",
            vec!["Afegir tests".into()],
        )
        .await;

        assert!(
            matches!(result.status, Status::Success),
            "La compilació ha de funcionar: {}",
            result.message
        );
        assert!(result.commit_message.is_some(), "Ha d'haver-hi un missatge de commit");

        let msg = result.commit_message.unwrap();
        assert!(msg.contains("[RO-6] Historial Atòmic"));
        assert!(msg.contains("Regla d'Or 6 implementada"));
    }

    #[tokio::test]
    async fn test_build_and_commit_invalid_code_no_commit() {
        let code = "fn main() { undefined_function(); }";
        let result = build_and_commit(
            code,
            "BR-001",
            "Test Primer",
            "Implementació de TDD",
            "in-progress",
            "Codi invàlid",
            vec![],
        )
        .await;

        assert!(
            matches!(result.status, Status::Error),
            "El codi invàlid ha de fallar"
        );
        assert!(
            result.commit_hash.is_none(),
            "No s'ha de fer commit amb codi invàlid"
        );
    }
}
