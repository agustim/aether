//! Mòdul de gestió de commits — Regla d'Or 6: Historial Atòmic
//!
//! Genera missatges de commit estandarditzats i crea commits
//! amb les dades de la regla de negoci, resultats de tests i
//! estat del Todo-Context.
//!
//! Utilitza el comandament `git` CLI per evitar dependre de l'API de git2.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

/// Resultats del conjunt de tests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResults {
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    #[serde(default)]
    pub details: Vec<String>,
}

impl TestResults {
    pub fn all_passed(total: u32) -> Self {
        Self {
            total,
            passed: total,
            failed: 0,
            details: Vec::new(),
        }
    }

    pub fn with_failures(total: u32, failed: u32, details: Vec<String>) -> Self {
        Self {
            total,
            passed: total - failed,
            failed,
            details,
        }
    }

    pub fn summary(&self) -> String {
        format!("{} passed, {} failed", self.passed, self.failed)
    }
}

/// Estat del Todo-Context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoContext {
    pub status: String,
    pub description: String,
    #[serde(default)]
    pub pending: Vec<String>,
}

impl TodoContext {
    pub fn new(status: &str, description: &str) -> Self {
        Self {
            status: status.into(),
            description: description.into(),
            pending: Vec::new(),
        }
    }

    pub fn add_pending(&mut self, task: &str) {
        self.pending.push(task.into());
    }

    pub fn short(&self) -> String {
        format!("[{}] {}", self.status, self.description)
    }
}

/// Metadata d'una regla de negoci implementada.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessRule {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// Dades completes per a generar un commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitMetadata {
    pub business_rule: BusinessRule,
    pub test_results: TestResults,
    pub todo_context: TodoContext,
}

/// Genera el missatge de commit estandarditzat segons la Regla d'Or 6.
pub fn generate_commit_message(meta: &CommitMetadata) -> String {
    let mut lines = Vec::new();

    let subject = format!(
        "feat: [{}] {} — {}",
        meta.business_rule.id,
        meta.business_rule.name,
        meta.business_rule.description
    );
    lines.push(subject);
    lines.push(String::new());
    lines.push(format!("Tests: {}", meta.test_results.summary()));

    if !meta.test_results.details.is_empty() {
        lines.push(String::new());
        lines.push("Test details:".into());
        for detail in &meta.test_results.details {
            lines.push(format!("  - {detail}"));
        }
    }

    lines.push(String::new());
    lines.push(format!("Todo-Context: {}", meta.todo_context.short()));

    if !meta.todo_context.pending.is_empty() {
        lines.push(String::new());
        lines.push("Pending tasks:".into());
        for (i, task) in meta.todo_context.pending.iter().enumerate() {
            lines.push(format!("  {}. {}", i + 1, task));
        }
    }

    lines.join("\n")
}

/// Configura el nom i email per al commit (només la primera vegada).
fn configure_git_user(repo_path: &PathBuf) -> Result<(), String> {
    // Configurar autor per defecte si no està configurat
    let output = Command::new("git")
        .args(["config", "user.name"])
        .current_dir(repo_path)
        .output()
        .ok();

    if output.as_ref().is_some_and(|o| o.status.success() && o.stdout.is_empty()) {
        Command::new("git")
            .args(["config", "user.name", "Aether Code"])
            .current_dir(repo_path)
            .output()
            .map_err(|e| format!("Error configurant user.name: {e}"))?;
    }

    let output = Command::new("git")
        .args(["config", "user.email"])
        .current_dir(repo_path)
        .output()
        .ok();

    if output.as_ref().is_some_and(|o| o.status.success() && o.stdout.is_empty()) {
        Command::new("git")
            .args(["config", "user.email", "aether@local"])
            .current_dir(repo_path)
            .output()
            .map_err(|e| format!("Error configurant user.email: {e}"))?;
    }

    Ok(())
}

/// Realitza un commit git amb el missatge generat.
/// Retorna el hash del commit (primeres 7 chars).
pub fn make_commit(repo_path: &PathBuf, message: &str) -> Result<String, String> {
    configure_git_user(repo_path)?;

    // 1. git add -A (tots els fitxers)
    let status = Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo_path)
        .status()
        .map_err(|e| format!("Error executant git add: {e}"))?;

    if !status.success() {
        return Err("git add ha fallat".into());
    }

    // 2. Comprovar si hi ha canvis amb git diff --cached --quiet
    let status = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(repo_path)
        .status()
        .map_err(|e| format!("Error executant git diff: {e}"))?;

    if status.success() {
        // Sense canvis (exit code 0 = sense diferències)
        return Err("No hi ha canvis per commitar".into());
    }

    // 3. Crear el commit
    let output = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Error executant git commit: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("git commit ha fallat: {stderr}"));
    }

    // 4. Obtenir el hash del commit més recent
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Error obtenint el hash: {e}"))?;

    let hash = String::from_utf8_lossy(&output.stdout)
        .trim()
        .chars()
        .take(7)
        .collect::<String>();

    Ok(hash)
}

/// Crea un fitxer Todo-Context si no existeix.
pub fn init_todo_context(repo_path: &PathBuf, description: &str) -> Result<(), String> {
    let todo_path = repo_path.join("todo-context.md");
    if todo_path.exists() {
        return Ok(());
    }

    let content = format!(
        "# Todo-Context — Aether Code\n\n\
        ## Estat actual\n\n\
        Status: init\n\
        Description: {description}\n\n\
        ## Tasques pendents\n\n\
        - [ ] Inicialitzar projecte\n"
    );

    std::fs::write(&todo_path, content)
        .map_err(|e| format!("No s'ha pogut crear todo-context.md: {e}"))
}

/// Actualitza el fitxer Todo-Context amb l'estat actual.
pub fn update_todo_context(
    repo_path: &PathBuf,
    todo: &TodoContext,
) -> Result<(), String> {
    let todo_path = repo_path.join("todo-context.md");

    let mut content = format!(
        "# Todo-Context — Aether Code\n\n\
        ## Estat actual\n\n\
        Status: {}\n\
        Description: {}\n",
        todo.status, todo.description
    );

    if !todo.pending.is_empty() {
        content.push_str("\n## Tasques pendents\n\n");
        for (i, task) in todo.pending.iter().enumerate() {
            content.push_str(&format!("- [ ] {}. {}\n", i + 1, task));
        }
    }

    std::fs::write(&todo_path, content)
        .map_err(|e| format!("No s'ha pogut actualitzar todo-context.md: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_results_summary_all_passed() {
        let results = TestResults::all_passed(5);
        assert_eq!(results.summary(), "5 passed, 0 failed");
        assert_eq!(results.total, 5);
        assert_eq!(results.passed, 5);
        assert_eq!(results.failed, 0);
    }

    #[test]
    fn test_test_results_summary_with_failures() {
        let results = TestResults::with_failures(5, 2, vec!["test_a".into(), "test_b".into()]);
        assert_eq!(results.summary(), "3 passed, 2 failed");
        assert_eq!(results.total, 5);
        assert_eq!(results.passed, 3);
        assert_eq!(results.failed, 2);
    }

    #[test]
    fn test_todo_context_short() {
        let mut todo = TodoContext::new("in-progress", "Implementant commit git");
        assert_eq!(todo.short(), "[in-progress] Implementant commit git");
        todo.add_pending("Afegir tests");
        assert_eq!(todo.pending.len(), 1);
    }

    #[test]
    fn test_generate_commit_message_basic() {
        let meta = CommitMetadata {
            business_rule: BusinessRule {
                id: "RO-6".into(),
                name: "Historial Atòmic".into(),
                description: "Commit automàtic amb missatge estandarditzat".into(),
            },
            test_results: TestResults::all_passed(5),
            todo_context: TodoContext::new("complete", "Regla d'Or 6 implementada"),
        };

        let msg = generate_commit_message(&meta);
        assert!(msg.starts_with("feat: [RO-6] Historial Atòmic — "));
        assert!(msg.contains("5 passed, 0 failed"));
        assert!(msg.contains("[complete] Regla d'Or 6 implementada"));
    }

    #[test]
    fn test_generate_commit_message_with_pending() {
        let mut todo = TodoContext::new("in-progress", "Desenvolupament en curs");
        todo.add_pending("Afegir suport per a Docker");
        todo.add_pending("Crear interfície web");

        let meta = CommitMetadata {
            business_rule: BusinessRule {
                id: "BR-001".into(),
                name: "Test Primer".into(),
                description: "Implementació de TDD obligatori".into(),
            },
            test_results: TestResults::with_failures(3, 1, vec!["test_auth".into()]),
            todo_context: todo,
        };

        let msg = generate_commit_message(&meta);
        assert!(msg.contains("2 passed, 1 failed"));
        assert!(msg.contains("[in-progress] Desenvolupament en curs"));
        assert!(msg.contains("Pending tasks:"));
        assert!(msg.contains("1. Afegir suport per a Docker"));
        assert!(msg.contains("2. Crear interfície web"));
    }

    #[test]
    fn test_generate_commit_message_no_pending() {
        let meta = CommitMetadata {
            business_rule: BusinessRule {
                id: "BR-002".into(),
                name: "Context Atòmic".into(),
                description: "Gestió de fitxer de context".into(),
            },
            test_results: TestResults::all_passed(3),
            todo_context: TodoContext::new("complete", "Context gestionat correctament"),
        };

        let msg = generate_commit_message(&meta);
        assert!(msg.contains("3 passed, 0 failed"));
        assert!(!msg.contains("Pending tasks:"));
    }

    #[test]
    fn test_init_todo_context() {
        let tmp = std::env::temp_dir().join("aether_test_todo");
        std::fs::create_dir_all(&tmp).unwrap();
        let _ = std::fs::remove_file(tmp.join("todo-context.md"));

        let result = init_todo_context(&tmp, "Test inicial");
        assert!(result.is_ok(), "Inicialitzar el context ha de funcionar");

        let todo_path = tmp.join("todo-context.md");
        assert!(todo_path.exists(), "El fitxer ha d'existir");

        let content = std::fs::read_to_string(&todo_path).unwrap();
        assert!(content.contains("Test inicial"));

        let _ = std::fs::remove_file(todo_path);
        let _ = std::fs::remove_dir(tmp);
    }

    #[test]
    fn test_update_todo_context() {
        let tmp = std::env::temp_dir().join("aether_test_update");
        std::fs::create_dir_all(&tmp).unwrap();

        let todo = TodoContext::new("in-progress", "Actualitzant context");
        let result = update_todo_context(&tmp, &todo);
        assert!(result.is_ok(), "Actualitzar el context ha de funcionar");

        let todo_path = tmp.join("todo-context.md");
        let content = std::fs::read_to_string(&todo_path).unwrap();
        assert!(content.contains("[in-progress]"));
        assert!(content.contains("Actualitzant context"));

        let _ = std::fs::remove_file(todo_path);
        let _ = std::fs::remove_dir(tmp);
    }
}
