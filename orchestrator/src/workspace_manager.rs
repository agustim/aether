//! Mòdul Workspace Manager — Gestió multi-projecte amb control d'accés
//!
//! Aquest mòdul gestiona:
//! - Creació i registre de workspaces (projectes aïllats)
//! - Validació de permisos per usuari (ACL)
//! - Validació de rutes segures contra directory traversal
//! - Configuració de recursos Docker per workspace

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// ============================================================================
// Models de dades
// ============================================================================

/// Permisos sobre un workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Permission {
    /// Lectura sola
    Read,
    /// Lectura i escriptura
    Write,
    /// Administració completa
    Admin,
}

/// Configuració de recursos per a un workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceResources {
    /// Límit de memòria en MB (per defecte 512)
    pub memory_mb: u32,
    /// Límit de CPU en fracció de core (per defecte 0.5)
    pub cpu_quota: f64,
}

impl Default for WorkspaceResources {
    fn default() -> Self {
        Self {
            memory_mb: 512,
            cpu_quota: 0.5,
        }
    }
}

/// Registre d'un workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub owner: String,
    pub allowed_users: HashMap<String, Permission>,
    pub path: PathBuf,
    pub resources: WorkspaceResources,
}

impl Workspace {
    /// Crea un nou workspace amb valors per defecte.
    pub fn new(id: &str, owner: &str, base_dir: &Path) -> Self {
        Self {
            id: id.to_string(),
            owner: owner.to_string(),
            allowed_users: {
                let mut map = HashMap::new();
                map.insert(owner.to_string(), Permission::Admin);
                map
            },
            path: base_dir.join(id),
            resources: WorkspaceResources::default(),
        }
    }
}

/// Registre global de workspaces (workspaces.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRegistry {
    pub workspaces: Vec<Workspace>,
}

impl Default for WorkspaceRegistry {
    fn default() -> Self {
        Self {
            workspaces: Vec::new(),
        }
    }
}

impl WorkspaceRegistry {
    /// Obté un workspace pel seu ID.
    pub fn get(&self, id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    /// Afegeix un workspace al registre.
    pub fn add(&mut self, workspace: Workspace) {
        self.workspaces.push(workspace);
    }

    /// Elimina un workspace del registre per ID.
    pub fn remove(&mut self, id: &str) -> Option<Workspace> {
        let pos = self.workspaces.iter().position(|w| w.id == id)?;
        Some(self.workspaces.remove(pos))
    }

    /// Llista tots els workspaces registrats.
    pub fn list_workspaces(&self) -> Vec<&Workspace> {
        self.workspaces.iter().collect()
    }

    /// Comprova si un usuari té accés a un workspace.
    pub fn check_permission(&self, workspace_id: &str, user_id: &str) -> bool {
        if let Some(ws) = self.get(workspace_id) {
            ws.allowed_users.contains_key(user_id)
        } else {
            false
        }
    }

    /// Obté el permís d'un usuari sobre un workspace.
    pub fn get_permission(&self, workspace_id: &str, user_id: &str) -> Option<Permission> {
        self.get(workspace_id)
            .and_then(|ws| ws.allowed_users.get(user_id).cloned())
    }
}

// ============================================================================
// WorkspaceManager
// ============================================================================

/// Gestor principal de workspaces.
pub struct WorkspaceManager {
    /// Directori base on es creen els workspaces
    base_dir: PathBuf,
    /// Ruta al fitxer de registre (workspaces.json)
    registry_path: PathBuf,
    /// Registre en memòria (es carrega/desava al fitxer quan cal)
    registry: WorkspaceRegistry,
}

impl WorkspaceManager {
    /// Crea un nou WorkspaceManager amb un directori base.
    ///
    /// # Arguments
    /// * `base_dir` — Directori on es crearan els workspaces (ex: `/storage`)
    pub fn new(base_dir: PathBuf) -> Self {
        let registry_path = base_dir.join("workspaces.json");
        let registry = if registry_path.exists() {
            match fs::read_to_string(&registry_path) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(reg) => reg,
                    Err(_) => WorkspaceRegistry::default(),
                },
                Err(_) => WorkspaceRegistry::default(),
            }
        } else {
            WorkspaceRegistry::default()
        };

        Self {
            base_dir,
            registry_path,
            registry,
        }
    }

    /// Crea un nou workspace i el registra.
    ///
    /// Crea el directori físic i el registre al fitxer.
    ///
    /// # Arguments
    /// * `id` — Identificador únic del workspace (ex: "factorial-lab")
    /// * `owner` — ID de l'usuari propietari
    ///
    /// # Errors
    /// Retorna un `String` amb el missatge d'error si no es pot crear.
    pub fn create_workspace(&mut self, id: &str, owner: &str) -> Result<Workspace, String> {
        // Validar que l'ID no contingui caràcters perillosos
        if !Self::is_valid_workspace_id(id) {
            return Err(format!(
                "ID de workspace invàlid: només alfanumèrics, guions i guions baixes. Got: {}",
                id
            ));
        }

        // Comprovar si el workspace ja existeix
        if self.registry.workspaces.iter().any(|w| w.id == id) {
            return Err(format!("El workspace '{}' ja existeix", id));
        }

        // Crear workspace
        let workspace = Workspace::new(id, owner, &self.base_dir);

        // Validar ruta
        let resolved = self.validate_path(&workspace.path)?;

        // Validar que la ruta és dins del base_dir
        if !resolved.starts_with(&self.base_dir) {
            return Err("Ruta fora del directori base".into());
        }

        // Crear directori físic
        fs::create_dir_all(&resolved)
            .map_err(|e| format!("Error creant directori: {e}"))?;

        // Registrar el workspace
        self.registry.add(workspace.clone());
        self.save_registry(&self.registry)?;

        Ok(workspace)
    }

    /// Obté un workspace pel seu ID.
    pub fn get_workspace(&self, id: &str) -> Option<&Workspace> {
        self.registry.get(id)
    }

    /// Comprova si un usuari té permís sobre un workspace.
    pub fn has_permission(&self, workspace_id: &str, user_id: &str) -> bool {
        self.registry.check_permission(workspace_id, user_id)
    }

    /// Obté el permís d'un usuari sobre un workspace.
    pub fn get_user_permission(
        &self,
        workspace_id: &str,
        user_id: &str,
    ) -> Option<Permission> {
        self.registry.get_permission(workspace_id, user_id)
    }

    /// Llista tots els workspaces registrats.
    pub fn list_workspaces(&self) -> Vec<&Workspace> {
        self.registry.list_workspaces()
    }

    /// Elimina un workspace del registre (sense eliminar el directori físic).
    pub fn remove_workspace(&mut self, id: &str) -> Result<Workspace, String> {
        if self.registry.get(id).is_none() {
            return Err(format!("El workspace '{}' no existeix", id));
        }

        // Modificar el registre existent
        let mut reg = self.registry.clone();
        match reg.remove(id) {
            Some(ws) => {
                self.save_registry(&reg)?;
                Ok(ws)
            }
            None => Err(format!("No s'ha pogut eliminar el workspace '{id}'")),
        }
    }

    /// Valida un path perquè no contingui directory traversal.
    ///
    /// Retorna el path resolt (amb .. eliminats).
    ///
    /// # Errors
    /// Retorna un error si el path conté patterns perillosos.
    pub fn validate_path(&self, path: &Path) -> Result<PathBuf, String> {
        let path_str = path.to_string_lossy();

        // Comprovar patterns de directory traversal
        if path_str.contains("..") {
            return Err(format!(
                "Ruta no permessa (conté '..'): {}",
                path_str
            ));
        }

        // Comprovar si el path conté .. en qualsevol component
        for component in path.components() {
            if let std::path::Component::Normal(c) = component {
                if c.to_string_lossy() == ".." {
                    return Err(format!(
                        "Ruta no permessa (component '..'): {}",
                        path_str
                    ));
                }
            }
        }

        // Resolver la ruta absoluta
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base_dir.join(path)
        };

        Ok(resolved)
    }

    /// Comprova si un ID de workspace és vàlid.
    ///
    /// Un ID és vàlid si només conté: alfanumèrics, guions (-), guions baixes (_).
    pub fn is_valid_workspace_id(id: &str) -> bool {
        id.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            && !id.is_empty()
    }

    /// Desava el registre al fitxer.
    pub fn save_registry(&self, registry: &WorkspaceRegistry) -> Result<(), String> {
        fs::create_dir_all(&self.base_dir)
            .map_err(|e| format!("Error creant directori base: {e}"))?;

        let content = serde_json::to_string_pretty(registry)
            .map_err(|e| format!("Error serialitzant registre: {e}"))?;

        fs::write(&self.registry_path, content)
            .map_err(|e| format!("Error escrivint registre: {e}"))?;

        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // Helper per crear un directori temporal per a tests
    fn temp_base() -> PathBuf {
        let dir = std::env::temp_dir().join("aether_workspace_test");
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn cleanup_temp(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }

    // ========================================================================
    // Tests de Workspace i WorkspaceRegistry
    // ========================================================================

    #[test]
    fn test_workspace_creation() {
        let base = temp_base().join("ws_test_1");
        let ws = Workspace::new("test-ws", "user_1", &base);

        assert_eq!(ws.id, "test-ws");
        assert_eq!(ws.owner, "user_1");
        assert!(ws.path.starts_with(&base));
        assert!(ws.allowed_users.contains_key("user_1"));
        assert_eq!(
            ws.allowed_users.get("user_1"),
            Some(&Permission::Admin)
        );
        cleanup_temp(&base);
    }

    #[test]
    fn test_workspace_registry_add_and_get() {
        let mut registry = WorkspaceRegistry::default();

        let base = temp_base().join("ws_reg_test");
        let ws = Workspace::new("my-ws", "owner", &base);
        registry.add(ws.clone());

        let found = registry.get("my-ws");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "my-ws");
        assert_eq!(found.unwrap().owner, "owner");

        let not_found = registry.get("nonexistent");
        assert!(not_found.is_none());

        cleanup_temp(&base);
    }

    #[test]
    fn test_workspace_registry_list() {
        let mut registry = WorkspaceRegistry::default();

        let base = temp_base().join("ws_list_test");
        registry.add(Workspace::new("ws-1", "user_a", &base));
        registry.add(Workspace::new("ws-2", "user_b", &base));

        let list = registry.list_workspaces();
        assert_eq!(list.len(), 2);
        let ids: Vec<&str> = list.iter().map(|w| w.id.as_str()).collect();
        assert!(ids.contains(&"ws-1"));
        assert!(ids.contains(&"ws-2"));

        cleanup_temp(&base);
    }

    #[test]
    fn test_workspace_registry_remove() {
        let mut registry = WorkspaceRegistry::default();
        let base = temp_base().join("ws_rm_test");

        registry.add(Workspace::new("ws-remove", "user_x", &base));
        assert_eq!(registry.workspaces.len(), 1);

        let removed = registry.remove("ws-remove");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, "ws-remove");
        assert_eq!(registry.workspaces.len(), 0);

        cleanup_temp(&base);
    }

    // ========================================================================
    // Tests de permisos (ACL)
    // ========================================================================

    #[test]
    fn test_permission_read() {
        let mut registry = WorkspaceRegistry::default();
        let base = temp_base().join("ws_perm_test");

        let mut ws = Workspace::new("perm-ws", "owner", &base);
        ws.allowed_users.insert("reader".into(), Permission::Read);
        ws.allowed_users.insert("writer".into(), Permission::Write);
        registry.add(ws);

        assert!(registry.check_permission("perm-ws", "owner"));
        assert!(registry.check_permission("perm-ws", "reader"));
        assert!(registry.check_permission("perm-ws", "writer"));
        assert!(!registry.check_permission("perm-ws", "noone"));

        assert_eq!(
            registry.get_permission("perm-ws", "reader"),
            Some(Permission::Read)
        );
        assert_eq!(
            registry.get_permission("perm-ws", "writer"),
            Some(Permission::Write)
        );
        assert_eq!(
            registry.get_permission("perm-ws", "noone"),
            None
        );

        cleanup_temp(&base);
    }

    #[test]
    fn test_owner_always_has_admin() {
        let base = temp_base().join("ws_owner_test");
        let ws = Workspace::new("owner-ws", "agusti", &base);

        assert!(registry_from_ws(ws.clone()).check_permission("owner-ws", "agusti"));
        assert_eq!(
            registry_from_ws(ws).get_permission("owner-ws", "agusti"),
            Some(Permission::Admin)
        );

        cleanup_temp(&base);
    }

    // Helper per crear un registry amb un sol workspace
    fn registry_from_ws(ws: Workspace) -> WorkspaceRegistry {
        let mut reg = WorkspaceRegistry::default();
        reg.add(ws);
        reg
    }

    // ========================================================================
    // Tests de validació de rutes
    // ========================================================================

    #[test]
    fn test_validate_path_safe() {
        let base = temp_base().join("ws_path_safe");
        let manager = WorkspaceManager::new(base.clone());

        let result = manager.validate_path(&base.join("safe-path"));
        assert!(result.is_ok());
        assert!(result.unwrap().starts_with(&base));

        cleanup_temp(&base);
    }

    #[test]
    fn test_validate_path_traversal_blocked() {
        let base = temp_base().join("ws_path_traversal");
        let manager = WorkspaceManager::new(base.clone());

        let result = manager.validate_path(&base.join("subdir").join("..").join("etc"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains(".."));

        cleanup_temp(&base);
    }

    #[test]
    fn test_validate_path_outside_base_blocked() {
        let base = temp_base().join("ws_path_outside");
        let manager = WorkspaceManager::new(base.clone());

        // Crear un path absolut fora del base
        let outside = PathBuf::from("/tmp/outside_workspace");
        let result = manager.validate_path(&outside);

        // El path no conté ".." però està fora del base_dir
        // La validació de "starts_with" es fa a create_workspace
        assert!(result.is_ok()); // validate_path només valida .., no la posició

        cleanup_temp(&base);
    }

    // ========================================================================
    // Tests de WorkspaceManager
    // ========================================================================

    #[test]
    fn test_workspace_manager_create_and_get() {
        let base = temp_base().join("ws_manager_create");
        let mut manager = WorkspaceManager::new(base.clone());

        let ws = manager.create_workspace("project-alpha", "alice").expect("Has de crear el workspace");
        assert_eq!(ws.id, "project-alpha");
        assert_eq!(ws.owner, "alice");
        assert!(ws.path.exists());

        let found = manager.get_workspace("project-alpha");
        assert!(found.is_some());
        assert_eq!(found.unwrap().owner, "alice");

        cleanup_temp(&base);
    }

    #[test]
    fn test_workspace_manager_duplicate_id() {
        let base = temp_base().join("ws_manager_dup");
        let mut manager = WorkspaceManager::new(base.clone());

        let _ = manager.create_workspace("dup-ws", "bob").expect("Primer intent OK");
        let result = manager.create_workspace("dup-ws", "carol");

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ja existeix"));

        cleanup_temp(&base);
    }

    #[test]
    fn test_workspace_manager_invalid_id() {
        let base = temp_base().join("ws_manager_invalid");
        let mut manager = WorkspaceManager::new(base.clone());

        let result = manager.create_workspace("../evil", "user");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invàlid"));

        let result = manager.create_workspace("evil/path", "user");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invàlid"));

        let result = manager.create_workspace("valid-id-123", "user");
        assert!(result.is_ok());

        cleanup_temp(&base);
    }

    #[test]
    fn test_workspace_manager_permission_check() {
        let base = temp_base().join("ws_manager_perm");
        let mut manager = WorkspaceManager::new(base.clone());

        manager.create_workspace("team-ws", "leader").expect("OK");

        // Només el líder té permís inicialment
        assert!(manager.has_permission("team-ws", "leader"));
        assert!(!manager.has_permission("team-ws", "stranger"));

        // Afegir un usuari permès modificant el registre en memòria
        if let Some(ws) = manager.registry.workspaces.iter_mut().find(|w| w.id == "team-ws") {
            ws.allowed_users.insert("member".into(), Permission::Write);
        }
        manager.save_registry(&manager.registry).unwrap();

        assert!(manager.has_permission("team-ws", "member"));
        assert_eq!(
            manager.get_user_permission("team-ws", "member"),
            Some(Permission::Write)
        );

        cleanup_temp(&base);
    }

    #[test]
    fn test_workspace_manager_remove() {
        let base = temp_base().join("ws_manager_rm");
        let mut manager = WorkspaceManager::new(base.clone());

        manager.create_workspace("to-remove", "user").expect("OK");
        assert!(manager.get_workspace("to-remove").is_some());

        let removed = manager.remove_workspace("to-remove").expect("OK");
        assert_eq!(removed.id, "to-remove");
        assert!(manager.get_workspace("to-remove").is_none());

        // Eliminar un workspace inexistent
        let result = manager.remove_workspace("nonexistent");
        assert!(result.is_err());

        cleanup_temp(&base);
    }

    #[test]
    fn test_workspace_manager_list() {
        let base = temp_base().join("ws_manager_list");
        let mut manager = WorkspaceManager::new(base.clone());

        manager.create_workspace("ws-a", "u1").expect("OK");
        manager.create_workspace("ws-b", "u2").expect("OK");
        manager.create_workspace("ws-c", "u3").expect("OK");

        let list = manager.list_workspaces();
        assert_eq!(list.len(), 3);
        let ids: Vec<&str> = list.iter().map(|w| w.id.as_str()).collect();
        assert!(ids.contains(&"ws-a"));
        assert!(ids.contains(&"ws-b"));
        assert!(ids.contains(&"ws-c"));

        cleanup_temp(&base);
    }

    #[test]
    fn test_is_valid_workspace_id() {
        assert!(WorkspaceManager::is_valid_workspace_id("valid-id"));
        assert!(WorkspaceManager::is_valid_workspace_id("valid_id"));
        assert!(WorkspaceManager::is_valid_workspace_id("valid123"));
        assert!(WorkspaceManager::is_valid_workspace_id("a"));
        assert!(!WorkspaceManager::is_valid_workspace_id(""));
        assert!(!WorkspaceManager::is_valid_workspace_id("invalid id"));
        assert!(!WorkspaceManager::is_valid_workspace_id("path/to/ws"));
        assert!(!WorkspaceManager::is_valid_workspace_id("../evil"));
    }

    #[test]
    fn test_workspace_registry_persistence() {
        let base = temp_base().join("ws_persistence");
        let mut manager = WorkspaceManager::new(base.clone());

        manager.create_workspace("persist-ws", "user").expect("OK");

        // Descarregar i tornar a carregar el registre des del fitxer
        let manager2 = WorkspaceManager::new(base.clone());
        let registry2 = manager2.registry;
        assert!(registry2.get("persist-ws").is_some());
        assert_eq!(registry2.get("persist-ws").unwrap().owner, "user");

        cleanup_temp(&base);
    }

    #[test]
    fn test_workspace_resources_defaults() {
        let ws = Workspace::new("res-ws", "user", Path::new("/tmp"));
        assert_eq!(ws.resources.memory_mb, 512);
        assert_eq!(ws.resources.cpu_quota, 0.5);
    }

    #[test]
    fn test_sandbox_volume_isolation() {
        // Test: verifica que cada workspace té el seu propi path
        // que pot ser muntat com a volum aïllat a Docker.
        let base = temp_base().join("ws_volume");
        let mut manager = WorkspaceManager::new(base.clone());

        manager.create_workspace("ws-1", "user1").expect("OK");
        manager.create_workspace("ws-2", "user2").expect("OK");

        let ws1 = manager.get_workspace("ws-1").unwrap();
        let ws2 = manager.get_workspace("ws-2").unwrap();

        assert!(!ws1.path.starts_with(ws2.path.as_path()));
        assert!(!ws2.path.starts_with(ws1.path.as_path()));
        assert!(ws1.path.parent().unwrap() == base);
        assert!(ws2.path.parent().unwrap() == base);

        // Verificar que els directors existeixen (preparats per al muntatge Docker)
        assert!(ws1.path.exists());
        assert!(ws2.path.exists());

        cleanup_temp(&base);
    }

    // ========================================================================
    // Tests de seguretat
    // ========================================================================

    #[test]
    fn test_workspace_path_traversal_in_registry() {
        // Test: verifica que les rutes del registre no escapen del base_dir.
        let base = temp_base().join("ws_security");
        let mut manager = WorkspaceManager::new(base.clone());

        // Intentar crear un workspace amb path traversal a la ruta
        let result = manager.create_workspace("evil-path", "attacker");
        assert!(result.is_ok()); // L'ID és vàlid

        let ws = manager.get_workspace("evil-path").unwrap();
        let resolved = manager.validate_path(&ws.path).unwrap();
        assert!(resolved.starts_with(&base),
            "La ruta resolta ha d'estar dins del base_dir");

        cleanup_temp(&base);
    }

    #[test]
    fn test_permission_denied_for_unknown_user() {
        let base = temp_base().join("ws_perm_denied");
        let mut manager = WorkspaceManager::new(base.clone());

        manager.create_workspace("secure-ws", "admin").expect("OK");

        assert!(!manager.has_permission("secure-ws", "unknown_user"));
        assert_eq!(
            manager.get_user_permission("secure-ws", "unknown_user"),
            None
        );

        cleanup_temp(&base);
    }
}
