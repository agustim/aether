//! Mòdul Telegram Bot — Interfície de xat asíncrona amb l'Orquestrador
//!
//! Aquest mòdul implementa un bot de Telegram que permet:
//! - Enviar intencions al workspace actiu (`/new_intent`)
//! - Veure l'estat del sistema (`/status`)
//! - Canviar de workspace (`/switch`)
//! - Veure logs del sandbox (`/logs`)
//! - Aprovar/rebutjar propostes de l'IA amb inline keyboards

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

// ============================================================================
// Models de dades
// ============================================================================

/// Tipus de comanda Telegram processada.
#[derive(Debug, Clone, PartialEq)]
pub enum TelegramCommand {
    /// /start — Benvinguda
    Start,
    /// /new_intent <text> — Nova intenció
    NewIntent(String),
    /// /status — Estat del sistema
    Status,
    /// /switch <workspace_id> — Canviar workspace
    SwitchWorkspace(String),
    /// /logs — Veure logs
    Logs,
    /// Resposta a callback_query (aprovació/rebuig)
    ApproveProposal(String),  // workspace_id + proposal_id
    RejectProposal(String),   // workspace_id + proposal_id
    /// Comanda no reconeguda
    Unknown(String),
}

/// Proposta d'intenció generada per l'IA.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentProposal {
    pub id: String,
    pub workspace_id: String,
    pub intent: String,
    pub explanation: String,
    pub tasks: Vec<String>,
    pub timestamp: String,
}

/// Chat registrat amb el seu estat actual.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredChat {
    /// ID de chat de Telegram (numèric)
    pub chat_id: i64,
    /// ID del workspace actiu
    pub active_workspace: Option<String>,
    /// Últim missatge enviat (per referència)
    pub last_message: Option<String>,
}

/// Configuració del bot de Telegram.
#[derive(Debug, Clone)]
pub struct TelegramConfig {
    /// Token del bot (obtingut de @BotFather)
    pub bot_token: String,
    /// IDs de Telegram autoritzats
    pub authorized_ids: Vec<String>,
}

impl TelegramConfig {
    /// Carrega la configuració des de les variables d'entorn.
    pub fn from_env() -> Result<Self, String> {
        let bot_token = std::env::var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| "TELEGRAM_BOT_TOKEN no configurada")?;

        let authorized_ids = std::env::var("TELEGRAM_AUTHORIZED_IDS")
            .unwrap_or_default()
            .split(|c: char| c == ',' || c == ' ' || c == '|')
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .collect();

        Ok(Self { bot_token, authorized_ids })
    }

    /// Verifica si un usuari està autoritzat.
    pub fn is_authorized(&self, user_id: &str) -> bool {
        self.authorized_ids.is_empty()
            || self.authorized_ids.iter().any(|id| id == user_id)
    }
}

// ============================================================================
// Parsing de comandes
// ============================================================================

/// Analitza un missatge de Telegram i retorna el comandament corresponent.
///
/// Els missatges poden ser:
/// - `/start` → Start
/// - `/new_intent text` → NewIntent(text)
/// - `/status` → Status
/// - `/switch workspace_id` → SwitchWorkspace(id)
/// - `/logs` → Logs
/// - Qualsevol altre text → NewIntent(text)
pub fn parse_telegram_command(message: &str) -> TelegramCommand {
    let message = message.trim();

    if message == "/start" {
        return TelegramCommand::Start;
    }

    // Verificar comanda exacta sense arguments
    if message == "/new_intent" {
        return TelegramCommand::Unknown("/new_intent requereix un text".into());
    }
    if let Some(text) = message.strip_prefix("/new_intent ").or_else(|| message.strip_prefix("/new_intent\t")) {
        return TelegramCommand::NewIntent(text.to_string());
    }

    if message == "/status" {
        return TelegramCommand::Status;
    }

    if message == "/switch" {
        return TelegramCommand::Unknown("/switch requereix un workspace_id".into());
    }
    if let Some(workspace_id) = message.strip_prefix("/switch ").or_else(|| message.strip_prefix("/switch\t")) {
        return TelegramCommand::SwitchWorkspace(workspace_id.to_string());
    }

    if message == "/logs" {
        return TelegramCommand::Logs;
    }

    // Si no és cap comanda, tractar com a nova intenció
    TelegramCommand::NewIntent(message.to_string())
}

// ============================================================================
// Generació de missatges i teclats
// ============================================================================

/// Construeix el missatge de benvinguda.
pub fn build_welcome_message(bot_name: &str) -> String {
    format!(
        "🤖 **{0}** està actiu!\n\n\
        Comandes disponibles:\n\
        `/new_intent <text>` — Enviar una intenció\n\
        `/status` — Veure l'estat del sistema\n\
        `/switch <workspace>` — Canviar de workspace\n\
        `/logs` — Veure logs del sandbox\n\n\
        💡 Tria un workspace amb `/switch` abans d'enviar intencions.",
        bot_name
    )
}

/// Construeix el resum visual de l'estat.
pub fn build_status_message(tasks: &[(&str, usize)]) -> String {
    let mut msg = "📊 **Estat del sistema**\n\n".to_string();

    for (label, count) in tasks {
        let emoji = match *label {
            "completed" => "✅",
            "pending" => "⏳",
            "failed" => "❌",
            "in_progress" => "🔄",
            _ => "⚪",
        };
        msg.push_str(&format!("{emoji} **{label}**: {count}\n"));
    }

    msg.push_str("\n👉 Fes `/new_intent` per proposar una tasca!");
    msg
}

/// Construeix la proposta amb inline keyboard d'aprovació.
pub fn build_proposal_message(proposal: &IntentProposal) -> (String, String) {
    let text = format!(
        "🤖 **Proposta d'IA**\n\n\
        💬 *{}*\n\
        📝 *{}*\n\n\
        Tasques:\n{}",
        proposal.intent,
        proposal.explanation,
        proposal.tasks.iter()
            .enumerate()
            .map(|(i, t)| format!("  {}. {t}", i + 1))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // El callback_data identifica la proposta i l'acció
    let callback_data = format!("{}_{}", proposal.workspace_id, proposal.id);

    (text, callback_data)
}

/// Construeix el missatge d'aprovació/rebuig.
pub fn build_action_message(action: &str, workspace_id: &str) -> String {
    let emoji = if action == "approve" { "✅" } else { "❌" };
    format!(
        "{emoji} Proposta {action} per al workspace `{workspace_id}`.\n\n\
        El sistema executarà la tasca automàticament.",
        action = action.to_lowercase(),
    )
}

// ============================================================================
// Persistència de chats
// ============================================================================

/// Gestor de chats registrats.
pub struct ChatRegistry {
    /// Mapa de chat_id → RegisteredChat
    pub chats: HashMap<i64, RegisteredChat>,
    /// Ruta al fitxer de registre
    pub registry_path: std::path::PathBuf,
}

impl ChatRegistry {
    /// Crea un nou registre de chats.
    pub fn new(base_dir: &Path) -> Self {
        Self {
            chats: HashMap::new(),
            registry_path: base_dir.join("telegram_chats.json"),
        }
    }

    /// Carrega els chats des del fitxer.
    pub fn load(base_dir: &Path) -> Self {
        let registry_path = base_dir.join("telegram_chats.json");
        if registry_path.exists() {
            if let Ok(content) = fs::read_to_string(&registry_path) {
                if let Ok(chats) = serde_json::from_str::<HashMap<i64, RegisteredChat>>(&content) {
                    return Self {
                        chats,
                        registry_path,
                    };
                }
            }
        }
        Self::new(base_dir)
    }

    /// Desa els chats al fitxer.
    pub fn save(&self) -> Result<(), String> {
        fs::create_dir_all(self.registry_path.parent().unwrap())
            .map_err(|e| format!("Error creant directori: {e}"))?;

        let content = serde_json::to_string_pretty(&self.chats)
            .map_err(|e| format!("Error serialitzant chats: {e}"))?;

        fs::write(&self.registry_path, content)
            .map_err(|e| format!("Error escrivint registre: {e}"))?;

        Ok(())
    }

    /// Registra un nou chat.
    pub fn register_chat(&mut self, chat_id: i64, workspace: Option<&str>) {
        self.chats.insert(chat_id, RegisteredChat {
            chat_id,
            active_workspace: workspace.map(String::from),
            last_message: None,
        });
        let _ = self.save();
    }

    /// Obté un chat registrat.
    pub fn get_chat(&self, chat_id: i64) -> Option<&RegisteredChat> {
        self.chats.get(&chat_id)
    }

    /// Obté el workspace actiu d'un chat.
    pub fn get_active_workspace(&self, chat_id: i64) -> Option<&str> {
        self.chats.get(&chat_id)
            .and_then(|c| c.active_workspace.as_deref())
    }

    /// Actualitza el workspace actiu d'un chat.
    pub fn set_active_workspace(&mut self, chat_id: i64, workspace: &str) {
        if let Some(chat) = self.chats.get_mut(&chat_id) {
            chat.active_workspace = Some(workspace.to_string());
        }
        let _ = self.save();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Tests de parsing de comandes
    // ========================================================================

    #[test]
    fn test_parse_command_start() {
        assert_eq!(parse_telegram_command("/start"), TelegramCommand::Start);
        assert_eq!(parse_telegram_command("/start "), TelegramCommand::Start);
    }

    #[test]
    fn test_parse_command_new_intent() {
        assert_eq!(
            parse_telegram_command("/new_intent Vull crear una funció factorial"),
            TelegramCommand::NewIntent("Vull crear una funció factorial".into())
        );
        assert_eq!(
            parse_telegram_command("/new_intent\tVull crear una funció factorial"),
            TelegramCommand::NewIntent("Vull crear una funció factorial".into())
        );
    }

    #[test]
    fn test_parse_command_new_intent_empty() {
        let result = parse_telegram_command("/new_intent");
        assert!(matches!(result, TelegramCommand::Unknown(_)));
    }

    #[test]
    fn test_parse_command_status() {
        assert_eq!(parse_telegram_command("/status"), TelegramCommand::Status);
    }

    #[test]
    fn test_parse_command_switch() {
        assert_eq!(
            parse_telegram_command("/switch factorial-lab"),
            TelegramCommand::SwitchWorkspace("factorial-lab".into())
        );
        assert_eq!(
            parse_telegram_command("/switch\tmy-project"),
            TelegramCommand::SwitchWorkspace("my-project".into())
        );
    }

    #[test]
    fn test_parse_command_switch_empty() {
        let result = parse_telegram_command("/switch");
        assert!(matches!(result, TelegramCommand::Unknown(_)));
    }

    #[test]
    fn test_parse_command_logs() {
        assert_eq!(parse_telegram_command("/logs"), TelegramCommand::Logs);
    }

    #[test]
    fn test_parse_command_as_intent() {
        // Qualsevol text sense prefix de comanda es tracta com a intenció
        assert_eq!(
            parse_telegram_command("Vull crear un servidor HTTP"),
            TelegramCommand::NewIntent("Vull crear un servidor HTTP".into())
        );
        assert_eq!(
            parse_telegram_command("Afegir autenticació al projecte"),
            TelegramCommand::NewIntent("Afegir autenticació al projecte".into())
        );
    }

    // ========================================================================
    // Tests de TelegramConfig
    // ========================================================================

    #[test]
    fn test_telegram_config_from_env_missing() {
        let saved_token = std::env::var("TELEGRAM_BOT_TOKEN").ok();

        std::env::remove_var("TELEGRAM_BOT_TOKEN");
        let result = TelegramConfig::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("TELEGRAM_BOT_TOKEN"));

        if let Some(val) = saved_token {
            std::env::set_var("TELEGRAM_BOT_TOKEN", val);
        }
    }

    #[test]
    fn test_telegram_config_is_authorized() {
        let config = TelegramConfig {
            bot_token: "fake-token".into(),
            authorized_ids: vec!["user1".into(), "user2".into()],
        };

        assert!(config.is_authorized("user1"));
        assert!(config.is_authorized("user2"));
        assert!(!config.is_authorized("user3"));
    }

    #[test]
    fn test_telegram_config_authorized_ids_parsing() {
        let config = TelegramConfig {
            bot_token: "fake-token".into(),
            authorized_ids: vec!["id1".into(), "id2".into(), "id3".into()],
        };

        assert!(config.is_authorized("id1"));
        assert!(config.is_authorized("id2"));
        assert!(config.is_authorized("id3"));
        assert!(!config.is_authorized("unknown"));
    }

    // ========================================================================
    // Tests de missatges i teclats
    // ========================================================================

    #[test]
    fn test_build_welcome_message() {
        let msg = build_welcome_message("AetherBot");
        assert!(msg.contains("AetherBot"));
        assert!(msg.contains("/new_intent"));
        assert!(msg.contains("/status"));
        assert!(msg.contains("/switch"));
        assert!(msg.contains("/logs"));
    }

    #[test]
    fn test_build_status_message() {
        let tasks = vec![
            ("completed", 5),
            ("pending", 3),
            ("failed", 1),
        ];
        let msg = build_status_message(&tasks);
        assert!(msg.contains("✅"));
        assert!(msg.contains("⏳"));
        assert!(msg.contains("❌"));
        assert!(msg.contains("completed"));
        assert!(msg.contains("pending"));
        assert!(msg.contains("failed"));
    }

    #[test]
    fn test_build_proposal_message() {
        let proposal = IntentProposal {
            id: "prop-1".into(),
            workspace_id: "test-ws".into(),
            intent: "Crear funció factorial".into(),
            explanation: "Implementar factorial recursiu".into(),
            tasks: vec![
                "Crear funció factorial".into(),
                "Afegir tests".into(),
            ],
            timestamp: "2024-01-01T00:00:00Z".into(),
        };

        let (text, callback_data) = build_proposal_message(&proposal);
        assert!(text.contains("Crear funció factorial"));
        assert!(text.contains("Implementar factorial recursiu"));
        assert!(callback_data.contains("test-ws"));
        assert!(callback_data.contains("prop-1"));
    }

    #[test]
    fn test_build_action_message_approve() {
        let msg = build_action_message("approve", "test-ws");
        assert!(msg.contains("✅"));
        assert!(msg.contains("approve"));
        assert!(msg.contains("test-ws"));
    }

    #[test]
    fn test_build_action_message_reject() {
        let msg = build_action_message("reject", "test-ws");
        assert!(msg.contains("❌"));
        assert!(msg.contains("reject"));
        assert!(msg.contains("test-ws"));
    }

    // ========================================================================
    // Tests de ChatRegistry
    // ========================================================================

    #[test]
    fn test_chat_registry_register_and_get() {
        let base = std::env::temp_dir().join("aether_telegram_test_1");
        let _ = fs::create_dir_all(&base);

        let mut registry = ChatRegistry::load(&base);
        let chat_id = 123456789;

        registry.register_chat(chat_id, Some("my-ws"));
        let chat = registry.get_chat(chat_id);
        assert!(chat.is_some());
        let chat = chat.unwrap();
        assert_eq!(chat.chat_id, chat_id);
        assert_eq!(chat.active_workspace.as_deref(), Some("my-ws"));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_chat_registry_get_active_workspace() {
        let base = std::env::temp_dir().join("aether_telegram_test_2");
        let _ = fs::create_dir_all(&base);

        let mut registry = ChatRegistry::load(&base);
        registry.register_chat(987654321, Some("ws-alpha"));

        assert_eq!(registry.get_active_workspace(987654321), Some("ws-alpha"));
        assert_eq!(registry.get_active_workspace(0), None);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_chat_registry_set_active_workspace() {
        let base = std::env::temp_dir().join("aether_telegram_test_3");
        let _ = fs::create_dir_all(&base);

        let mut registry = ChatRegistry::load(&base);
        registry.register_chat(111222333, Some("ws-old"));
        registry.set_active_workspace(111222333, "ws-new");

        assert_eq!(registry.get_active_workspace(111222333), Some("ws-new"));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_chat_registry_persistence() {
        let base = std::env::temp_dir().join("aether_telegram_test_4");
        let _ = fs::create_dir_all(&base);

        let mut registry = ChatRegistry::load(&base);
        registry.register_chat(444555666, Some("persist-ws"));
        registry.save().expect("Ha de desar el registre");

        // Recarregar des d'un registre diferent
        let registry2 = ChatRegistry::load(&base);
        assert!(registry2.get_chat(444555666).is_some());
        assert_eq!(
            registry2.get_active_workspace(444555666),
            Some("persist-ws")
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_chat_registry_unregistered_chat() {
        let base = std::env::temp_dir().join("aether_telegram_test_5");
        let _ = fs::create_dir_all(&base);

        let registry = ChatRegistry::load(&base);
        assert!(registry.get_chat(999999999).is_none());
        assert_eq!(registry.get_active_workspace(999999999), None);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_parse_command_with_special_characters() {
        // Comanda amb caràcters especials
        assert_eq!(
            parse_telegram_command("/new_intent Crear un servidor HTTP/2 amb TLS"),
            TelegramCommand::NewIntent("Crear un servidor HTTP/2 amb TLS".into())
        );
    }
}
