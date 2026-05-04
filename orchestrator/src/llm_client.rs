//! Client de LLM — Connexió a models de llenguatge via API OpenAI-compatible
//!
//! Aquest mòdul gestiona la comunicació amb un LLM extern (Qwen, GPT-4, etc.)
//! utilitzant una interfície compatible amb l'API d'OpenAI.
//!
//! Variables d'entorn necessàries:
//! - AETHER_LLM_URL: URL base de l'API (ex: https://api.openai.com/v1)
//! - AETHER_LLM_KEY: Clau d'autenticació (Bearer Token)
//! - AETHER_LLM_MODEL: Nom del model (ex: gpt-4, qwen-2.5-coder)

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

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

/// Construeix una petició al LLM.
pub fn build_llm_request(
    config: &LLMConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> LLMRequest {
    LLMRequest {
        model: config.model.clone(),
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
        temperature: Some(0.3), // Lower temperature for more consistent outputs
    }
}

/// Executa una crida al LLM.
pub async fn call_llm(config: &LLMConfig, system_prompt: &str, user_prompt: &str) -> Result<LLMResult, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Error creant client HTTP: {e}"))?;

    let request = build_llm_request(config, system_prompt, user_prompt);

    let url = format!("{}/chat/completions", config.url.trim_end_matches('/'));

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await;

    match response {
        Ok(resp) => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();

            if status.is_success() {
                // Parsejar la resposta
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
        Err(e) => Ok(LLMResult {
            success: false,
            content: String::new(),
            error: Some(format!("Error de xarxa: {e}")),
        }),
    }
}

/// Parseja la resposta del LLM com a JSON.
pub fn parse_llm_response(response: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(response)
        .map_err(|e| format!("Error parsejant JSON del LLM: {e}"))
}

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
        // Guardar valor actual (si existeix)
        let url = std::env::var("AETHER_LLM_URL").ok();
        let key = std::env::var("AETHER_LLM_KEY").ok();

        // Eliminar variables
        std::env::remove_var("AETHER_LLM_URL");
        std::env::remove_var("AETHER_LLM_KEY");

        let result = LLMConfig::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("AETHER_LLM_URL"));

        // Restaurar valors
        if let Some(val) = url {
            std::env::set_var("AETHER_LLM_URL", val);
        }
        if let Some(val) = key {
            std::env::set_var("AETHER_LLM_KEY", val);
        }
    }

    #[test]
    fn test_llm_config_from_env_missing_key() {
        // Guardar valors actuals
        let url = std::env::var("AETHER_LLM_URL").ok();
        let key = std::env::var("AETHER_LLM_KEY").ok();

        // Posar només URL, eliminar KEY
        std::env::set_var("AETHER_LLM_URL", "https://api.test.com/v1");
        std::env::remove_var("AETHER_LLM_KEY");

        let result = LLMConfig::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("AETHER_LLM_KEY"));

        // Restaurar valors
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
    fn test_llm_connectivity_mock_server() {
        // Test de connectivitat: verifica que el client LLM pot connectar
        // a un servidor mock i rebre una resposta vàlida.

        // Crear servidor mock
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"choices": [{"message": {"role": "assistant", "content": "{\"explanation\":\"OK\",\"tasks\":[{\"id\":1,\"description\":\"test\",\"status\":\"pending\"}]}"}}]}"#)
            .create();

        // Configurar variables d'entorn
        let url = format!("{}/v1", server.url());
        let saved_url = std::env::var("AETHER_LLM_URL").ok();
        let saved_key = std::env::var("AETHER_LLM_KEY").ok();
        let saved_model = std::env::var("AETHER_LLM_MODEL").ok();

        std::env::set_var("AETHER_LLM_URL", url);
        std::env::set_var("AETHER_LLM_KEY", "mock-key-for-testing");
        std::env::set_var("AETHER_LLM_MODEL", "test-model");

        // Executar crida
        let config = LLMConfig::from_env().expect("Config ha de ser vàlida");
        let result = rt().block_on(async {
            call_llm(&config, "system prompt", "user prompt").await
        });

        // Restaurar variables
        if let Some(val) = saved_url {
            std::env::set_var("AETHER_LLM_URL", val);
        } else {
            std::env::remove_var("AETHER_LLM_URL");
        }
        if let Some(val) = saved_key {
            std::env::set_var("AETHER_LLM_KEY", val);
        } else {
            std::env::remove_var("AETHER_LLM_KEY");
        }
        if let Some(val) = saved_model {
            std::env::set_var("AETHER_LLM_MODEL", val);
        } else {
            std::env::remove_var("AETHER_LLM_MODEL");
        }

        // Verificar resultats
        assert!(result.is_ok(), "La crida al LLM ha de funcionar amb servidor mock");
        let llm_result = result.unwrap();
        assert!(llm_result.success, "El LLM ha de retornar success, error: {:?}", llm_result.error);
        assert!(llm_result.content.contains("explanation"));
        assert!(llm_result.content.contains("tasks"));
        mock.assert();
    }

    #[test]
    fn test_llm_malformed_json_response() {
        // Test de parsing: verifica que si el LLM retorna JSON mal format,
        // l'orquestrador ho gestiona sense panic.

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

        let config = LLMConfig::from_env().expect("Config ha de ser vàlida");
        let result = rt().block_on(async {
            call_llm(&config, "system prompt", "user prompt").await
        });

        // Restaurar variables
        if let Some(val) = saved_url {
            std::env::set_var("AETHER_LLM_URL", val);
        } else {
            std::env::remove_var("AETHER_LLM_URL");
        }
        if let Some(val) = saved_key {
            std::env::set_var("AETHER_LLM_KEY", val);
        } else {
            std::env::remove_var("AETHER_LLM_KEY");
        }
        if let Some(val) = saved_model {
            std::env::set_var("AETHER_LLM_MODEL", val);
        } else {
            std::env::remove_var("AETHER_LLM_MODEL");
        }

        // La crida HTTP és exitosa (200) — l'objectiu és que no faci panic
        assert!(result.is_ok(), "La crida HTTP ha de ser OK");
        let llm_result = result.unwrap();
        assert!(llm_result.success, "HTTP 200 = success (el parsing no afecta això), error: {:?}", llm_result.error);
        // El content és el text brut (no JSON vàlid)
        assert!(llm_result.content.contains("això no és JSON"));
        mock.assert();
    }

    #[test]
    fn test_llm_http_error_response() {
        // Test: verifica que errors HTTP no-success es gestionen correctament.

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

        let config = LLMConfig::from_env().expect("Config ha de ser vàlida");
        let result = rt().block_on(async {
            call_llm(&config, "system prompt", "user prompt").await
        });

        // Restaurar variables
        if let Some(val) = saved_url {
            std::env::set_var("AETHER_LLM_URL", val);
        } else {
            std::env::remove_var("AETHER_LLM_URL");
        }
        if let Some(val) = saved_key {
            std::env::set_var("AETHER_LLM_KEY", val);
        } else {
            std::env::remove_var("AETHER_LLM_KEY");
        }
        if let Some(val) = saved_model {
            std::env::set_var("AETHER_LLM_MODEL", val);
        } else {
            std::env::remove_var("AETHER_LLM_MODEL");
        }

        // La crida retorna un LLMResult amb success=false
        assert!(result.is_ok(), "La crida ha de retornar un Result");
        let llm_result = result.unwrap();
        assert!(!llm_result.success, "HTTP 401 ha de marcar failure");
        assert!(llm_result.error.is_some(), "Ha de tenir missatge d'error");
        let err_msg = llm_result.error.as_ref().unwrap();
        assert!(err_msg.contains("401") || err_msg.contains("Invalid API key"),
            "L'error ha de contenir informació de l'error HTTP, got: {}", err_msg);
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
