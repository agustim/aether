//! Client de LLM — Connexió a models de llenguatge via API OpenAI-compatible
//!
//! Aquest mòdul gestiona la comunicació amb un LLM extern (Qwen, GPT-4, etc.)
//! utilitzant una interfície compatible amb l'API d'OpenAI.
//!
//! Variables d'entorn necessàries:
//! - AETHER_LLM_URL: URL base de l'API (ex: https://api.openai.com/v1)
//! - AETHER_LLM_KEY: Clau d'autenticació (Bearer Token)
//! - AETHER_LLM_MODEL: Nom del model (ex: gpt-4, qwen-2.5-coder)

use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ============================================================================
// Configuració i estructures internes
// ============================================================================

/// Configuració del client LLM.
#[derive(Debug, Clone)]
pub struct LLMConfig {
    pub url: String,
    pub api_key: String,
    pub model: String,
}

/// Petició al LLM (format OpenAI).
#[derive(Debug, Serialize)]
struct LLMRequest {
    model: String,
    messages: Vec<LLMMessage>,
    response_format: Option<LLMResponseFormat>,
    temperature: Option<f32>,
}

/// Missatge per al LLM.
#[derive(Debug, Serialize, Deserialize)]
struct LLMMessage {
    role: String,
    content: String,
}

/// Format de resposta esperat.
#[derive(Debug, Serialize)]
struct LLMResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
}

/// Resposta del LLM.
#[derive(Debug, Deserialize)]
struct LLMResponse {
    pub choices: Vec<LLMChoice>,
}

/// Opció de resposta.
#[derive(Debug, Deserialize)]
struct LLMChoice {
    pub message: LLMMessage,
}

/// Resultat de la crida al LLM.
#[derive(Debug)]
pub struct LLMResult {
    pub success: bool,
    pub content: String,
    pub error: Option<String>,
}

// ============================================================================
// LLMClient — Gestiona headers, format JSON i crides HTTP
// ============================================================================

/// Client de LLM que encapsula la connexió HTTP, headers i serialització JSON.
///
/// Aquesta estructura gestiona:
/// - La configuració de l'API (URL, clau, model)
/// - Els headers d'autorització (Bearer Token)
/// - El format JSON de les peticions i respostes
/// - El timeout de les crides HTTP
pub struct LLMClient {
    http_client: HttpClient,
    config: LLMConfig,
}

impl LLMClient {
    /// Crea un nou client LLM a partir de les variables d'entorn.
    ///
    /// # Errors
    /// Retorna un error si `AETHER_LLM_URL` o `AETHER_LLM_KEY` no estan definides.
    pub fn from_env() -> Result<Self, String> {
        let config = LLMConfig::from_env()?;
        Ok(Self::new(config))
    }

    /// Crea un nou client LLM amb una configuració específica.
    pub fn new(config: LLMConfig) -> Self {
        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Error creant client HTTP (timeout fix de 30s)");

        Self { http_client, config }
    }

    /// Construeix els headers de la petició amb el Bearer Token.
    fn build_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", self.config.api_key)
                .parse()
                .unwrap(),
        );
        headers
    }

    /// Construeix la petició JSON en format OpenAI.
    fn build_request_json(&self, system_prompt: &str, user_prompt: &str) -> LLMRequest {
        LLMRequest {
            model: self.config.model.clone(),
            messages: vec![
                LLMMessage {
                    role: "system".into(),
                    content: system_prompt.into(),
                },
                LLMMessage {
                    role: "user".into(),
                    content: user_prompt.into(),
                },
            ],
            response_format: Some(LLMResponseFormat {
                format_type: "json_object".into(),
            }),
            temperature: Some(0.3),
        }
    }

    /// Construeix la URL completa de l'endpoint.
    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.config.url.trim_end_matches('/'))
    }

    /// Envia la petició HTTP al LLM i parseja la resposta.
    async fn send_request(&self, system_prompt: &str, user_prompt: &str) -> Result<LLMResult, String> {
        let url = self.chat_url();
        let headers = self.build_headers();
        let body = self.build_request_json(system_prompt, user_prompt);

        let response = self
            .http_client
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await;

        match response {
            Ok(resp) => self.parse_response(resp).await,
            Err(e) => Ok(LLMResult {
                success: false,
                content: String::new(),
                error: Some(format!("Error de xarxa: {e}")),
            }),
        }
    }

    /// Parseja la resposta HTTP en un LLMResult.
    async fn parse_response(&self, resp: reqwest::Response) -> Result<LLMResult, String> {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();

        if status.is_success() {
            match serde_json::from_str::<LLMResponse>(&text) {
                Ok(llm_resp) => {
                    if let Some(choice) = llm_resp.choices.first() {
                        Ok(LLMResult {
                            success: true,
                            content: choice.message.content.clone(),
                            error: None,
                        })
                    } else {
                        Ok(LLMResult {
                            success: false,
                            content: String::new(),
                            error: Some("Resposta buida del LLM".into()),
                        })
                    }
                }
                Err(e) => Ok(LLMResult {
                    success: false,
                    content: String::new(),
                    error: Some(format!("Error parsejant resposta LLM: {e}")),
                }),
            }
        } else {
            Ok(LLMResult {
                success: false,
                content: String::new(),
                error: Some(format!("Error HTTP {}: {}", status, text)),
            })
        }
    }

    /// Realitza una crida al LLM amb system i user prompt.
    pub async fn call(&self, system_prompt: &str, user_prompt: &str) -> Result<LLMResult, String> {
        self.send_request(system_prompt, user_prompt).await
    }
}

// ============================================================================
// Funcions auxiliars públiques (mantingudes per compatibilitat)
// ============================================================================

impl LLMConfig {
    /// Crea una nova configuració des de les variables d'entorn.
    pub fn from_env() -> Result<Self, String> {
        let url = std::env::var("AETHER_LLM_URL")
            .map_err(|_| "AETHER_LLM_URL no configurada. Configura la URL de l'API LLM.")?;

        let api_key = std::env::var("AETHER_LLM_KEY")
            .map_err(|_| "AETHER_LLM_KEY no configurada. Configura la clau d'API.")?;

        let model = std::env::var("AETHER_LLM_MODEL")
            .unwrap_or_else(|_| "gpt-4".into());

        Ok(Self { url, api_key, model })
    }

    /// Verifica que la configuració és vàlida.
    pub fn is_valid(&self) -> bool {
        !self.url.is_empty() && !self.api_key.is_empty() && !self.model.is_empty()
    }
}

/// Construeix el prompt del sistema.
pub fn build_system_prompt() -> String {
    r#"Eres un Expert en Rust i Arquitectura de Sistemes. La teva tasca és transformar intencions d'usuari en tasques tècniques concretes.

Regles:
- Només respon en format JSON vàlid
- No incloguis markdown ni text addicional
- Cada tasca ha de ser implementable per un desenvolupador
- Retorna un array de tasques amb: id, description i status ("pending")

Format de resposta:
{
  "explanation": "Breu explicació de l'apropament",
  "tasks": [
    {
      "id": 1,
      "description": "Descripció tècnica detallada de la tasca",
      "status": "pending"
    }
  ]
}"#
    .into()
}

/// Construeix el prompt de context amb el todo-context.json actual.
pub fn build_context_prompt(current_context: &str) -> String {
    format!(
        "## Context Actual del Projecte\n\n{}\n\n\
        ### Instruccions\n\n\
        Analitza la intenció de l'usuari i genera una llista de tasques tècniques.\n\
        Considera les tasques existents per evitar duplicats.",
        current_context
    )
}

/// Funció auxiliar: executa una crida al LLM (equivalent a `LLMClient::call`).
pub async fn call_llm(config: &LLMConfig, system_prompt: &str, user_prompt: &str) -> Result<LLMResult, String> {
    let client = LLMClient::new(config.clone());
    client.call(system_prompt, user_prompt).await
}

/// Parseja la resposta del LLM com a JSON.
pub fn parse_llm_response(response: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(response)
        .map_err(|e| format!("Error parsejant JSON del LLM: {e}"))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_system_prompt_not_empty() {
        let prompt = build_system_prompt();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("Rust"));
        assert!(prompt.contains("JSON"));
    }

    #[test]
    fn test_build_context_prompt() {
        let context = r#"{ "tasks": [] }"#;
        let prompt = build_context_prompt(context);
        assert!(prompt.contains("Context Actual"));
        assert!(prompt.contains(context));
    }

    #[test]
    fn test_llm_config_from_env_missing() {
        let url = std::env::var("AETHER_LLM_URL").ok();
        let key = std::env::var("AETHER_LLM_KEY").ok();

        std::env::remove_var("AETHER_LLM_URL");
        std::env::remove_var("AETHER_LLM_KEY");

        let result = LLMConfig::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("AETHER_LLM_URL"));

        if let Some(val) = url {
            std::env::set_var("AETHER_LLM_URL", val);
        }
        if let Some(val) = key {
            std::env::set_var("AETHER_LLM_KEY", val);
        }
    }

    #[test]
    fn test_llm_config_from_env_missing_key() {
        let url = std::env::var("AETHER_LLM_URL").ok();
        let key = std::env::var("AETHER_LLM_KEY").ok();

        std::env::set_var("AETHER_LLM_URL", "https://api.test.com/v1");
        std::env::remove_var("AETHER_LLM_KEY");

        let result = LLMConfig::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("AETHER_LLM_KEY"));

        if let Some(val) = url {
            std::env::set_var("AETHER_LLM_URL", val);
        }
        if let Some(val) = key {
            std::env::set_var("AETHER_LLM_KEY", val);
        }
    }

    #[test]
    fn test_llm_config_is_valid() {
        let config = LLMConfig {
            url: "https://api.example.com/v1".into(),
            api_key: "test-key".into(),
            model: "gpt-4".into(),
        };
        assert!(config.is_valid());

        let invalid = LLMConfig {
            url: "".into(),
            api_key: "test".into(),
            model: "gpt-4".into(),
        };
        assert!(!invalid.is_valid());
    }

    #[test]
    fn test_llm_client_creation_from_config() {
        let config = LLMConfig {
            url: "https://api.test.com/v1".into(),
            api_key: "test-key".into(),
            model: "test-model".into(),
        };
        let client = LLMClient::new(config.clone());
        assert!(client.config.is_valid());
    }

    #[test]
    fn test_parse_llm_response_valid_json() {
        let json = r#"{"explanation": "test", "tasks": [{"id": 1, "description": "task", "status": "pending"}]}"#;
        let result = parse_llm_response(json);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_llm_response_invalid_json() {
        let json = r#"not valid json"#;
        let result = parse_llm_response(json);
        assert!(result.is_err());
    }

    // ========================================================================
    // Tests d'integració — requereixen mockito (dev-dependency)
    // ========================================================================

    #[test]
    fn test_llm_client_connectivity_mock() {
        // Test de connectivitat amb LLMClient (estructura)
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"choices": [{"message": {"role": "assistant", "content": "{\"explanation\":\"OK\",\"tasks\":[{\"id\":1,\"description\":\"test\",\"status\":\"pending\"}]}"}}]}"#)
            .create();

        let url = format!("{}/v1", server.url());
        let saved_url = std::env::var("AETHER_LLM_URL").ok();
        let saved_key = std::env::var("AETHER_LLM_KEY").ok();
        let saved_model = std::env::var("AETHER_LLM_MODEL").ok();

        std::env::set_var("AETHER_LLM_URL", url);
        std::env::set_var("AETHER_LLM_KEY", "mock-key-for-testing");
        std::env::set_var("AETHER_LLM_MODEL", "test-model");

        let client = LLMClient::from_env().expect("Config ha de ser vàlida");
        let result = rt().block_on(async {
            client.call("system prompt", "user prompt").await
        });

        if let Some(val) = saved_url { std::env::set_var("AETHER_LLM_URL", val); } else { std::env::remove_var("AETHER_LLM_URL"); }
        if let Some(val) = saved_key { std::env::set_var("AETHER_LLM_KEY", val); } else { std::env::remove_var("AETHER_LLM_KEY"); }
        if let Some(val) = saved_model { std::env::set_var("AETHER_LLM_MODEL", val); } else { std::env::remove_var("AETHER_LLM_MODEL"); }

        assert!(result.is_ok(), "La crida al LLM ha de funcionar amb LLMClient");
        let llm_result = result.unwrap();
        assert!(llm_result.success, "success: {:?}", llm_result.error);
        assert!(llm_result.content.contains("explanation"));
        mock.assert();
    }

    #[test]
    fn test_llm_malformed_json_response() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"choices": [{"message": {"role": "assistant", "content": "això no és JSON vàlid!!!"}}]}"#)
            .create();

        let url = format!("{}/v1", server.url());
        let saved_url = std::env::var("AETHER_LLM_URL").ok();
        let saved_key = std::env::var("AETHER_LLM_KEY").ok();
        let saved_model = std::env::var("AETHER_LLM_MODEL").ok();

        std::env::set_var("AETHER_LLM_URL", url);
        std::env::set_var("AETHER_LLM_KEY", "mock-key-for-testing");
        std::env::set_var("AETHER_LLM_MODEL", "test-model");

        let client = LLMClient::from_env().expect("Config ha de ser vàlida");
        let result = rt().block_on(async {
            client.call("system prompt", "user prompt").await
        });

        if let Some(val) = saved_url { std::env::set_var("AETHER_LLM_URL", val); } else { std::env::remove_var("AETHER_LLM_URL"); }
        if let Some(val) = saved_key { std::env::set_var("AETHER_LLM_KEY", val); } else { std::env::remove_var("AETHER_LLM_KEY"); }
        if let Some(val) = saved_model { std::env::set_var("AETHER_LLM_MODEL", val); } else { std::env::remove_var("AETHER_LLM_MODEL"); }

        assert!(result.is_ok());
        let llm_result = result.unwrap();
        assert!(llm_result.success, "error: {:?}", llm_result.error);
        assert!(llm_result.content.contains("això no és JSON"));
        mock.assert();
    }

    #[test]
    fn test_llm_http_error_response() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error": {"message": "Invalid API key"}}"#)
            .create();

        let url = format!("{}/v1", server.url());
        let saved_url = std::env::var("AETHER_LLM_URL").ok();
        let saved_key = std::env::var("AETHER_LLM_KEY").ok();
        let saved_model = std::env::var("AETHER_LLM_MODEL").ok();

        std::env::set_var("AETHER_LLM_URL", url);
        std::env::set_var("AETHER_LLM_KEY", "wrong-key");
        std::env::set_var("AETHER_LLM_MODEL", "test-model");

        let client = LLMClient::from_env().expect("Config ha de ser vàlida");
        let result = rt().block_on(async {
            client.call("system prompt", "user prompt").await
        });

        if let Some(val) = saved_url { std::env::set_var("AETHER_LLM_URL", val); } else { std::env::remove_var("AETHER_LLM_URL"); }
        if let Some(val) = saved_key { std::env::set_var("AETHER_LLM_KEY", val); } else { std::env::remove_var("AETHER_LLM_KEY"); }
        if let Some(val) = saved_model { std::env::set_var("AETHER_LLM_MODEL", val); } else { std::env::remove_var("AETHER_LLM_MODEL"); }

        assert!(result.is_ok());
        let llm_result = result.unwrap();
        assert!(!llm_result.success, "HTTP 401 ha de marcar failure");
        assert!(llm_result.error.is_some());
        let err_msg = llm_result.error.as_ref().unwrap();
        assert!(err_msg.contains("401") || err_msg.contains("Invalid API key"),
            "got: {}", err_msg);
        mock.assert();
    }

    // Helper per executar codi async en tests síncrons
    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Ha de funcionar crear el runtime de tokio")
    }
}
