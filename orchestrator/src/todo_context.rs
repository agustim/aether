//! Mòdul de gestió del fitxer todo-context.json
//!
//! Regla d'Or 3: Aquest fitxer és l'única font de veritat per a l'estat del projecte.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Tasca individual del projecte.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Task {
    pub id: u32,
    pub description: String,
    pub status: TaskStatus,
}

/// Estat d'una tasca.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}

impl TaskStatus {
    pub fn is_completed(&self) -> bool {
        matches!(self, TaskStatus::Completed)
    }

    pub fn is_in_progress(&self) -> bool {
        matches!(self, TaskStatus::InProgress)
    }
}

/// Context complet del projecte.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoContext {
    pub project_name: String,
    pub current_stage: String,
    pub tasks: Vec<Task>,
}

impl TodoContext {
    /// Crea un context buit.
    pub fn new(project_name: &str) -> Self {
        Self {
            project_name: project_name.into(),
            current_stage: "Dev".into(),
            tasks: Vec::new(),
        }
    }

    /// Afegeix una nova tasca. Retorna el nou ID.
    pub fn add_task(&mut self, description: &str) -> u32 {
        let new_id = self.tasks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        self.tasks.push(Task {
            id: new_id,
            description: description.into(),
            status: TaskStatus::Pending,
        });
        new_id
    }

  /// Canvia l'estat actual (Arquitecte, Analista, QA, Dev).
    pub fn set_stage(&mut self, stage: &str) {
        self.current_stage = stage.into();
    }

    /// Marca la tasca in_progress com a completed. Retorna el nombre de tasques canviades.
    pub fn complete_current_task(&mut self) -> u32 {
        let mut count = 0u32;
        for task in &mut self.tasks {
            if task.status == TaskStatus::InProgress {
                task.status = TaskStatus::Completed;
                count += 1;
            }
        }
        count
    }

    /// Retorna la primera tasca in_progress, si n'hi ha.
    pub fn get_current_task(&self) -> Option<&Task> {
        self.tasks.iter().find(|t| t.status == TaskStatus::InProgress)
    }
}

/// Ruta del fitxer todo-context.json.
fn context_path(repo_path: &Path) -> PathBuf {
    repo_path.join("todo-context.json")
}

/// Carrega el context des del fitxer.
pub fn load_context(repo_path: &Path) -> Result<TodoContext, String> {
    let path = context_path(repo_path);
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("No s'ha pogut llegir {path:?}: {e}"))?;

    serde_json::from_str(&content)
        .map_err(|e| format!("JSON invàlid a {path:?}: {e}"))
}

/// Guarda el context al fitxer.
pub fn save_context(repo_path: &Path, context: &TodoContext) -> Result<(), String> {
    let path = context_path(repo_path);
    let content = serde_json::to_string_pretty(context)
        .map_err(|e| format!("Error serialitzant context: {e}"))?;

    std::fs::write(&path, content)
        .map_err(|e| format!("No s'ha pogut escriure {path:?}: {e}"))
}

/// Inicialitza un fitxer de context si no existeix.
pub fn init_context_if_missing(repo_path: &Path) -> Result<(), String> {
    let path = context_path(repo_path);
    if path.exists() {
        return Ok(());
    }

    let context = TodoContext::new("Aether Code");
    save_context(repo_path, &context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_new_context() {
        let ctx = TodoContext::new("Test Project");
        assert_eq!(ctx.project_name, "Test Project");
        assert_eq!(ctx.current_stage, "Dev");
        assert!(ctx.tasks.is_empty());
    }

    #[test]
    fn test_add_task() {
        let mut ctx = TodoContext::new("Test");
        let id1 = ctx.add_task("Task 1");
        assert_eq!(id1, 1);
        assert_eq!(ctx.tasks[0].status, TaskStatus::Pending);

        let id2 = ctx.add_task("Task 2");
        assert_eq!(id2, 2);
        assert_eq!(ctx.tasks.len(), 2);
    }

    #[test]
    fn test_set_stage() {
        let mut ctx = TodoContext::new("Test");
        ctx.set_stage("Arquitecte");
        assert_eq!(ctx.current_stage, "Arquitecte");
    }

    #[test]
    fn test_complete_current_task() {
        let mut ctx = TodoContext::new("Test");
        ctx.add_task("Tasca 1");
        ctx.add_task("Tasca 2");

        // Marcar la tasca 2 com a in_progress
        ctx.tasks[1].status = TaskStatus::InProgress;

        let count = ctx.complete_current_task();
        assert_eq!(count, 1);
        assert!(ctx.tasks[1].status.is_completed());
    }

    #[test]
    fn test_complete_no_in_progress() {
        let mut ctx = TodoContext::new("Test");
        ctx.add_task("Tasca 1");
        // Totes pending

        let count = ctx.complete_current_task();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_get_current_task() {
        let mut ctx = TodoContext::new("Test");
        ctx.add_task("Tasca 1");
        ctx.add_task("Tasca 2");

        assert!(ctx.get_current_task().is_none());

        ctx.tasks[0].status = TaskStatus::InProgress;
        let current = ctx.get_current_task().unwrap();
        assert_eq!(current.id, 1);
        assert_eq!(current.description, "Tasca 1");
    }

    #[test]
    fn test_save_and_load_context() {
        let tmp = std::env::temp_dir().join("aether_test_context");
        std::fs::create_dir_all(&tmp).unwrap();

        let mut ctx = TodoContext::new("Test Project");
        ctx.add_task("Task A");
        ctx.add_task("Task B");
        ctx.tasks[0].status = TaskStatus::Completed;
        ctx.tasks[1].status = TaskStatus::InProgress;

        // Guardar
        let result = save_context(&tmp, &ctx);
        assert!(result.is_ok(), "Guardar context ha de funcionar");

        // Carregar
        let loaded = load_context(&tmp);
        assert!(loaded.is_ok(), "Carregar context ha de funcionar");

        let loaded = loaded.unwrap();
        assert_eq!(loaded.project_name, "Test Project");
        assert_eq!(loaded.tasks.len(), 2);
        assert_eq!(loaded.tasks[0].status, TaskStatus::Completed);
        assert_eq!(loaded.tasks[1].status, TaskStatus::InProgress);

        // Netejar
        let _ = std::fs::remove_file(tmp.join("todo-context.json"));
        let _ = std::fs::remove_dir(tmp);
    }

    #[test]
    fn test_init_context_if_missing() {
        let tmp = std::env::temp_dir().join("aether_test_init");
        std::fs::create_dir_all(&tmp).unwrap();
        let _ = std::fs::remove_file(tmp.join("todo-context.json"));

        let result = init_context_if_missing(&tmp);
        assert!(result.is_ok(), "Inicialitzar context ha de funcionar");

        let path = tmp.join("todo-context.json");
        assert!(path.exists(), "El fitxer ha d'existir");

        // Si ja existeix, no falla
        let result2 = init_context_if_missing(&tmp);
        assert!(result2.is_ok(), "Reinicialitzar no ha de fallar");

        // Netejar
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir(tmp);
    }
}
