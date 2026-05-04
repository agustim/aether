//! Aether Code — Compiler-Agent orchestrator
//!
//! Rebi un bloc de codi Rust, l'escriu a `sandbox/src/main.rs`
//! i executem `cargo check` per validar-lo.
//! Tot retorna un JSON amb status i missatge.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::process::Command;
use tokio::sync::Mutex;

pub mod commit;
pub mod todo_context;
pub mod docker_sandbox;
pub mod intent_analyst;
use todo_context::TaskStatus;

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
    #[serde(default = "default_rule_id")]
    rule_id: String,
    #[serde(default = "default_rule_name")]
    rule_name: String,
    #[serde(default = "default_rule_description")]
    rule_description: String,
    #[serde(default = "default_todo_status")]
    todo_status: String,
    #[serde(default = "default_todo_description")]
    todo_description: String,
    #[serde(default)]
    pending_tasks: Vec<String>,
}

fn default_rule_id() -> String { "BR-000".into() }
fn default_rule_name() -> String { "Compilació Simple".into() }
fn default_rule_description() -> String { "Codi enviat via HTTP sense metadades".into() }
fn default_todo_status() -> String { "in-progress".into() }
fn default_todo_description() -> String { "Compilació via HTTP".into() }

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

/// Completa la tasca in_progress, actualitza estats i guarda el context.
/// Retorna el nombre de tasques completades.
async fn complete_task_in_context(repo_path: &Path, todo_status: &str) -> u32 {
    // Carregar context
    let mut context = match todo_context::load_context(repo_path) {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("Avís: no s'ha pogut carregar el context: {e}");
            return 0;
        }
    };

    // Completar tasca in_progress i comptar
    let completed_task_count = context.complete_current_task();

    // Sempre marcar la primera tasca pending com a in_progress
    for task in &mut context.tasks {
        if task.status == TaskStatus::Pending {
            task.status = TaskStatus::InProgress;
            break;
        }
    }

    // Actualitzar el stage si s'ha especificat
    if todo_status != "in-progress" {
        context.set_stage(todo_status);
    }

    // Guardar el context actualitzat
    if let Err(e) = todo_context::save_context(repo_path, &context) {
        eprintln!("Avís: no s'ha pogut guardar el context: {e}");
    }

    completed_task_count
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
    #[serde(skip_serializing_if = "serde_json::Value::is_null", default)]
    completed_tasks: serde_json::Value,
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

    // Inicialitzar el fitxer de context si no existeix
    let _ = todo_context::init_context_if_missing(&repo_path);

    // Inicialitzar el sandbox Docker (construir imatge si no existeix)
    let config = docker_sandbox::DockerSandboxConfig {
        sandbox_dir: repo_path.join("sandbox"),
        cargo_cache: PathBuf::from("/tmp/aether_cargo_cache"),
        ..Default::default()
    };

    // Verificar si Docker està disponible
    let docker_available = docker_sandbox::docker_image_exists(&config.image_name).unwrap_or(false);

    // Comprovació prèvia: verificar que el codi no intenti accedir a xarxa (només si Docker està disponible)
    if docker_available {
        if let Err(e) = docker_sandbox::check_no_network_code(code) {
            return BuildResult {
                status: Status::Error,
                message: format!("Seguretat: {e}"),
                commit_hash: None,
                commit_message: None,
                completed_tasks: serde_json::Value::Null,
            };
        }
    }

    // Executar cargo check dins del contenidor Docker
    let docker_result = match docker_sandbox::run_docker_check(&config, &repo_path, code) {
        Ok(result) => result,
        Err(e) => {
            // Si Docker no està disponible, fallback a cargo check local
            eprintln!("Avís: Docker no disponible, utilitzant fallback local: {e}");
            // Inicialitzar el sandbox per al fallback
            if let Err(write_err) = write_sandbox_code(code).await {
                return BuildResult {
                    status: Status::Error,
                    message: format!("Error d'escriptura: {write_err}"),
                    commit_hash: None,
                    commit_message: None,
                    completed_tasks: serde_json::Value::Null,
                };
            }
            
            // Executar cargo check local
            if let Err(output) = run_cargo_check().await {
                return BuildResult {
                    status: Status::Error,
                    message: format!("Error en fallback local: {output}"),
                    commit_hash: None,
                    commit_message: None,
                    completed_tasks: serde_json::Value::Null,
                };
            }

            // Fallback local èxit — completar tasques i fer commit (Regla 6)
            let completed_task_count = complete_task_in_context(&repo_path, todo_status).await;

            // Generar meta de commit (sense test results en mode fallback)
            let test_results = commit::TestResults::all_passed(0);
            let mut todo_context = commit::TodoContext::new(todo_status, todo_description);
            for task in &pending_tasks {
                todo_context.add_pending(task);
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
            let commit_hash = match commit::make_commit_with_git2(&repo_path, &commit_msg) {
                Ok(hash) => {
                    let _ = commit::update_todo_context_file(&repo_path, &meta.todo_context);
                    Some(hash)
                }
                Err(e) => {
                    eprintln!("Avís: no s'ha pogut crear commit al fallback: {e}");
                    None
                }
            };

            return BuildResult {
                status: Status::Success,
                message: format!("Compilació correcta (fallback local). Docker no disponible: {e}"),
                commit_hash,
                commit_message: Some(commit_msg),
                completed_tasks: serde_json::json!({ "completed": completed_task_count }),
            };
        }
    };

    if !docker_result.success {
        return BuildResult {
            status: Status::Error,
            message: docker_result.output,
            commit_hash: None,
            commit_message: None,
            completed_tasks: serde_json::Value::Null,
        };
    }

    // Compilar el codi al sandbox físic
    let lock = get_sandbox_lock();
    let _guard = lock.lock().await;

    if let Err(e) = write_sandbox_code(code).await {
        return BuildResult {
            status: Status::Error,
            message: format!("Error d'escriptura: {e}"),
            commit_hash: None,
            commit_message: None,
            completed_tasks: serde_json::Value::Null,
        };
    }

    // Completar tasques i fer commit
    let completed_task_count = complete_task_in_context(&repo_path, todo_status).await;

    // Generar meta de commit (sense test results en mode Docker)
    let test_results = commit::TestResults::all_passed(0);

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
    let commit_hash = match commit::make_commit_with_git2(&repo_path, &commit_msg) {
        Ok(hash) => {
            // Actualitzar el Todo-Context
            let _ = commit::update_todo_context_file(&repo_path, &meta.todo_context);
            Some(hash)
        }
        Err(e) => {
            eprintln!("Avís: no s'ha pogut crear commit: {e}");
            None
        }
    };

    BuildResult {
        status: Status::Success,
        message: "Compilació correcta — docker check ha passat sense errors.".into(),
        commit_hash,
        commit_message: Some(commit_msg),
        completed_tasks: serde_json::json!({
            "completed": completed_task_count
        }),
    }
}

/// Parseja els resultats de tests de l'output de `cargo test` o `cargo check`.
/// Utilitzat en tests antics — mantingut per compatibilitat.
#[allow(dead_code)]
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

/// Construeix el router d'axum amb els endpoints.
fn build_router() -> axum::Router {
    use axum::routing::{get, post};
    use tower_http::trace::TraceLayer;

    axum::Router::new()
        .route("/compile", post(compile_handler))
        .route("/context", get(get_context_handler))
        .route("/context/task", post(add_task_handler))
        .route("/context/approve", post(approve_handler))
        .route("/docker/build", post(docker_build_handler))
        .route("/intent", post(intent_handler))
        .layer(TraceLayer::new_for_http())
}

/// Handler per POST /intent — rep una intenció i genera una proposta.
async fn intent_handler(
    axum::extract::Json(payload): axum::extract::Json<intent_analyst::IntentRequest>,
) -> axum::response::Json<serde_json::Value> {
    let repo_path = workspace_root();

    // Carregar context actual
    let context_path = repo_path.join("todo-context.json");
    let context_text = match std::fs::read_to_string(&context_path) {
        Ok(text) => text,
        Err(e) => {
            return axum::response::Json(serde_json::json!({
                "status": "error",
                "message": format!("Error carregant context: {}", e),
                "proposal": null
            }));
        }
    };

    // Generar proposta (utilitzar mock per defecte)
    let use_mock = std::env::var("AI_USE_MOCK").unwrap_or_else(|_| "true".into()) == "true";
    
    match intent_analyst::generate_proposal(&payload.intent, &context_text, use_mock) {
        Ok(proposal) => {
            // Guardar la proposta a proposals.json
            let proposals_path = repo_path.join("proposals.json");
            let proposals_text = std::fs::read_to_string(&proposals_path).unwrap_or_else(|_| "[]".into());
            let mut proposals: Vec<intent_analyst::IntentProposal> = serde_json::from_str(&proposals_text).unwrap_or_else(|_| Vec::new());
            proposals.push(proposal.clone());
            std::fs::write(&proposals_path, serde_json::to_string_pretty(&proposals).unwrap()).ok();
            
            axum::response::Json(serde_json::to_value(&proposal).unwrap())
        }
        Err(e) => axum::response::Json(serde_json::json!({
            "status": "error",
            "message": format!("Error generant proposta: {}", e),
            "proposal": null
        })),
    }
}

/// Handler per POST /context/approve — aprova una proposta.
async fn approve_handler(
    axum::extract::Json(payload): axum::extract::Json<intent_analyst::ApproveRequest>,
) -> axum::response::Json<serde_json::Value> {
    let repo_path = workspace_root();
    let context_path = repo_path.join("todo-context.json");

    // Carregar totes les propostes guardades
    let proposals_path = repo_path.join("proposals.json");
    let proposals_text = std::fs::read_to_string(&proposals_path).unwrap_or_else(|_| "[]".into());
    
    let proposals: Vec<intent_analyst::IntentProposal> = serde_json::from_str(&proposals_text)
        .unwrap_or_else(|_| Vec::new());

    // Trobar la proposta a aprovar
    let proposal = match proposals.iter().find(|p| p.proposal_id == payload.proposal_id) {
        Some(p) => p.clone(),
        None => {
            return axum::response::Json(serde_json::json!({
                "status": "error",
                "message": format!("Proposta #{} no trobada", payload.proposal_id),
                "tasks_added": 0
            }));
        }
    };

    // Aprovar la proposta
    match intent_analyst::approve_proposal(&proposal, &context_path) {
        Ok(response) => axum::response::Json(serde_json::to_value(&response).unwrap()),
        Err(e) => axum::response::Json(serde_json::json!({
            "status": "error",
            "message": format!("Error aprovant: {}", e),
            "tasks_added": 0
        })),
    }
}

/// Handler per construir la imatge Docker.
async fn docker_build_handler() -> axum::response::Json<serde_json::Value> {
    let repo_path = workspace_root();
    let config = docker_sandbox::DockerSandboxConfig {
        sandbox_dir: repo_path.join("sandbox"),
        ..Default::default()
    };

    match docker_sandbox::build_docker_image(&config, &repo_path) {
        Ok(()) => axum::response::Json(serde_json::json!({
            "message": "Imatge Docker construïda correctament",
            "image": config.image_name
        })),
        Err(e) => axum::response::Json(serde_json::json!({
            "error": format!("Error construint imatge: {}", e)
        })),
    }
}

/// Handler de l'endpoint POST /compile.
async fn compile_handler(
    axum::extract::Json(payload): axum::extract::Json<BuildRequest>,
) -> axum::response::Json<serde_json::Value> {
    let result = build_and_commit(
        &payload.code,
        &payload.rule_id,
        &payload.rule_name,
        &payload.rule_description,
        &payload.todo_status,
        &payload.todo_description,
        payload.pending_tasks,
    )
    .await;

    axum::response::Json(serde_json::to_value(&result).unwrap())
}

/// Handler de GET /context — retorna el context actual del projecte.
async fn get_context_handler() -> axum::response::Json<serde_json::Value> {
    let repo_path = workspace_root();

    // Inicialitzar si no existeix
    let _ = todo_context::init_context_if_missing(&repo_path);

    // Carregar context
    match todo_context::load_context(&repo_path) {
        Ok(context) => {
            axum::response::Json(serde_json::to_value(&context).unwrap())
        }
        Err(e) => {
            // Retornar error com a JSON (el caller gestiona el status)
            axum::response::Json(serde_json::json!({ "error": e }))
        }
    }
}

/// Petició per afegir una tasca.
#[derive(Debug, Deserialize)]
struct AddTaskRequest {
    description: String,
}

/// Handler de POST /context/task — afegeix una nova tasca.
async fn add_task_handler(
    axum::extract::Json(payload): axum::extract::Json<AddTaskRequest>,
) -> axum::response::Json<serde_json::Value> {
    let repo_path = workspace_root();

    // Inicialitzar si no existeix
    let _ = todo_context::init_context_if_missing(&repo_path);

    // Carregar context
    let mut context = match todo_context::load_context(&repo_path) {
        Ok(ctx) => ctx,
        Err(e) => {
            return axum::response::Json(serde_json::json!({
                "error": format!("Error carregant context: {}", e)
            }));
        }
    };

    // Afegir tasca
    let new_id = context.add_task(&payload.description);

    // Guardar
    if let Err(e) = todo_context::save_context(&repo_path, &context) {
        return axum::response::Json(serde_json::json!({
            "error": format!("Error guardant context: {}", e)
        }));
    }

    axum::response::Json(serde_json::json!({
        "message": "Tasca afegida correctament",
        "task_id": new_id,
        "task": context.tasks.iter().find(|t| t.id == new_id).unwrap()
    }))
}

fn main() {
    // Mode HTTP: si hi ha una variable d'ambient PORT, executa el servidor HTTP.
    // Sinó, mode consola (stdin → stdout) per compatibilitat.
    let rt = tokio::runtime::Runtime::new().expect("No s'ha pogut crear el runtime de tokio");

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());

    if std::env::var("MODE").unwrap_or_else(|_| "http".to_string()) == "http" {
        rt.block_on(async {
            let app = build_router();
            let addr = format!("0.0.0.0:{port}");
            let listener = tokio::net::TcpListener::bind(&addr)
                .await
                .expect("No s'ha pogut enllaçar el port");

            eprintln!("🛰️  Aether Orchestrator corrent a http://{addr}");
            eprintln!("   Endpoint: POST http://{addr}/compile");

            axum::serve(
                listener,
                app,
            )
            .await
            .expect("Error executant el servidor");
        });
    } else {
        // Mode consola per compatibilitat
        mode_console(&rt);
    }
}

/// Mode consola: llegir de stdin, escriure a stdout.
fn mode_console(_rt: &tokio::runtime::Runtime) {
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
        .read_to_string(&mut input)
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
                        completed_tasks: serde_json::Value::Null,
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
        // Regla 6: sempre ha d'haver-hi commit (fins i tot en fallback)
        assert!(result.commit_message.is_some(), "Ha d'haver-hi commit (Regla 6)");
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

    // ========================================================================
    // Tests HTTP — Regla de Negoci: endpoint /compile
    // ========================================================================

    #[tokio::test]
    async fn test_http_compile_valid_code() {
        // Arrencar el servidor en un port aleatori
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let app = build_router();

        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // Esperar que el servidor arrenqui
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Fer el POST
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{port}/compile");
        let response = client
            .post(&url)
            .json(&serde_json::json!({
                "code": "fn main() { println!(\"HTTP OK!\"); }"
            }))
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());

        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], "Success");

        // Aturar el servidor
        server.abort();
        let _ = server.await;
    }

    // ========================================================================
    // Test d'integració complet — Regla d'Or 3: Context Atòmic
    // ========================================================================

    #[tokio::test]
    async fn test_http_full_context_workflow() {
        // Preparar: crear un context amb tasques
        let repo_path = workspace_root();
        let json_path = repo_path.join("todo-context.json");

        // Escriure un context inicial amb una tasca pending
        let initial_context = serde_json::json!({
            "project_name": "Aether Code",
            "current_stage": "Dev",
            "tasks": [
                { "id": 1, "description": "Tasca antiga", "status": "completed" },
                { "id": 2, "description": "Tasca en curs", "status": "in_progress" },
                { "id": 3, "description": "Tasca futura", "status": "pending" }
            ]
        });
        std::fs::write(&json_path, serde_json::to_string_pretty(&initial_context).unwrap())
            .unwrap();

        // Arrencar el servidor
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let app = build_router();

        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let client = reqwest::Client::new();
        let base_url = format!("http://127.0.0.1:{port}");

        // 1. Verificar GET /context
        let response = client
            .get(format!("{base_url}/context"))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());

        let context: serde_json::Value = response.json().await.unwrap();
        assert_eq!(context["project_name"], "Aether Code");
        assert_eq!(context["tasks"].as_array().unwrap().len(), 3);

        // 2. Afegir una nova tasca amb POST /context/task
        let response = client
            .post(format!("{base_url}/context/task"))
            .json(&serde_json::json!({
                "description": "Nova tasca des del mòbil"
            }))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());

        let add_response: serde_json::Value = response.json().await.unwrap();
        assert_eq!(add_response["message"], "Tasca afegida correctament");
        let new_task_id = add_response["task_id"].as_u64().unwrap();
        assert_eq!(new_task_id, 4);

        // 3. Marcar la tasca 3 (pending) com a in_progress
        let mut ctx = serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(&json_path).unwrap()
        ).unwrap();
        ctx["tasks"][2]["status"] = serde_json::json!("in_progress");
        std::fs::write(&json_path, serde_json::to_string_pretty(&ctx).unwrap()).unwrap();

        // 4. Enviar codi vàlid a /compile
        let response = client
            .post(format!("{base_url}/compile"))
            .json(&serde_json::json!({
                "code": "fn main() { println!(\"Workflow OK!\"); }",
                "todo_status": "QA"
            }))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());

        let compile_response: serde_json::Value = response.json().await.unwrap();
        assert_eq!(compile_response["status"], "Success");

        // 5. Verificar que la tasca in_progress s'ha completat
        let context_response = client
            .get(format!("{base_url}/context"))
            .send()
            .await
            .unwrap();
        let final_context: serde_json::Value = context_response.json().await.unwrap();

        // La tasca 3 (in_progress) hauria de ser completed
        let task3_status = &final_context["tasks"][2]["status"];
        assert_eq!(task3_status, "completed", "La tasca in_progress ha de ser completed");

        // La tasca 4 (pending) hauria de ser in_progress
        let task4_status = &final_context["tasks"][3]["status"];
        assert_eq!(task4_status, "in_progress", "La primera tasca pending ha de ser in_progress");

        // El stage s'ha actualitzat a QA
        assert_eq!(final_context["current_stage"], "QA");

        // Netejar
        let _ = std::fs::remove_file(&json_path);
        server.abort();
        let _ = server.await;
    }
}
