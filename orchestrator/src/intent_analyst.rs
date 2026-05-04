//! Mòdul d'Analista d'Intencions
//!
//! Transforma 'Intencions' lliures de l'usuari en 'Tasques' tècniques
//! utilitzant IA (Qwen) amb el context actual del projecte.
//!
//! Flux:
//! 1. Usuari envia POST /intent amb text lliure
//! 2. Analista genera proposta de tasques tècniques
//! 3. Usuari aprova via POST /context/approve
//! 4. Tasques s'afegeixen al context

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Petició per enviar una intenció.
#[derive(Debug, Deserialize)]
pub struct IntentRequest {
    /// Text lliure de la intenció de l'usuari
    pub intent: String,
}

/// Tasca tècnica generada per l'IA.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnicalTask {
    /// ID únic de la tasca
    pub id: u32,
    /// Descripció tècnica de la tasca
    pub description: String,
    /// Estat de la tasca (proposal/in_progress/completed)
    pub status: TaskStatus,
}

/// Estat d'una tasca.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Proposal,
    Pending,
    InProgress,
    Completed,
}

/// Proposta generada per l'analista.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentProposal {
    /// ID de la proposta
    pub proposal_id: u32,
    /// Intenció original de l'usuari
    pub original_intent: String,
    /// Llista de tasques tècniques proposades
    pub tasks: Vec<TechnicalTask>,
    /// Missatge d'explicació
    pub explanation: String,
}

/// Resposta del POST /intent.
#[derive(Debug, Serialize)]
pub struct IntentResponse {
    /// Estat de la resposta
    pub status: String,
    /// Missatge descriptiu
    pub message: String,
    /// Proposta generada (si es va crear)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal: Option<IntentProposal>,
}

/// Aprovació de tasques.
#[derive(Debug, Deserialize)]
pub struct ApproveRequest {
    /// ID de la proposta a aprovar
    pub proposal_id: u32,
}

/// Resposta d'aprovació.
#[derive(Debug, Serialize)]
pub struct ApproveResponse {
    pub status: String,
    pub message: String,
    pub tasks_added: u32,
}

/// Genera una proposta de tasques a partir d'una intenció.
///
/// Aquesta funció simula la crida a l'IA (Qwen). En producció,
/// faria una crida real a l'API. Per a tests, utilitza un mock.
pub fn generate_proposal(
    intent: &str,
    current_context: &str,
    use_mock: bool,
) -> Result<IntentProposal, String> {
    if use_mock {
        generate_mock_proposal(intent, current_context)
    } else {
        generate_ai_proposal(intent, current_context)
    }
}

/// Versió real amb IA (Qwen API).
fn generate_ai_proposal(intent: &str, _context: &str) -> Result<IntentProposal, String> {
    let _api_key = std::env::var("AI_API_KEY").map_err(|_| {
        "AI_API_KEY no configurada. Configura la clau d'API de Qwen.".to_string()
    })?;

    // Construir el prompt per a l'IA
    let _prompt = format!(
        "Eres un analista tècnic expert en Rust. Transforma la següent intenció d'usuari \
        en tasques tècniques concretes i implementables.\n\n\
        Intenció: {}\n\n\
        Context actual del projecte:\n{}\n\n\
        Retorna només un JSON amb aquest format:\n\
        {{\n  \"explanation\": \"explicació breu\",\n  \"tasks\": [\n    {{\n      \"description\": \"descripció tècnica\",\n      \"status\": \"pending\"\n    }}\n  ]\n}}",
        intent, _context
    );

    // Simular crida a l'API (aqui aniria la crida real a Qwen)
    // Per ara, retornem una resposta mock que es pot substituir
    generate_mock_proposal(intent, _context)
}

/// Versió mock per a tests.
fn generate_mock_proposal(
    intent: &str,
    _context: &str,
) -> Result<IntentProposal, String> {
    // Generar tasques basades en paraules clau de la intenció
    let intent_lower = intent.to_lowercase();
    let mut tasks = Vec::new();
    let mut proposal_id = 1u32;

    // Parsejar la intenció i generar tasques genèriques
    if intent_lower.contains("saludar") || intent_lower.contains("hello") || intent_lower.contains("hola") {
        tasks.push(TechnicalTask {
            id: proposal_id,
            description: "Crear funció saludar() que retorni 'Hola món' en català".into(),
            status: TaskStatus::Proposal,
        });
        proposal_id += 1;
        tasks.push(TechnicalTask {
            id: proposal_id,
            description: "Crear test per validar la funció salutació".into(),
            status: TaskStatus::Proposal,
        });
    } else if intent_lower.contains("api") || intent_lower.contains("endpoint") {
        tasks.push(TechnicalTask {
            id: proposal_id,
            description: "Definir estructura de petició i resposta JSON".into(),
            status: TaskStatus::Proposal,
        });
        proposal_id += 1;
        tasks.push(TechnicalTask {
            id: proposal_id,
            description: "Implementar handler de l'endpoint amb axum".into(),
            status: TaskStatus::Proposal,
        });
        tasks.push(TechnicalTask {
            id: proposal_id,
            description: "Afegir tests d'integració per l'endpoint".into(),
            status: TaskStatus::Proposal,
        });
    } else {
        // Proposta genèrica per a qualsevol intenció
        tasks.push(TechnicalTask {
            id: proposal_id,
            description: format!("Analitzar requisits de: {}", intent),
            status: TaskStatus::Proposal,
        });
        proposal_id += 1;
        tasks.push(TechnicalTask {
            id: proposal_id,
            description: format!("Implementar funcionalitat: {}", intent),
            status: TaskStatus::Proposal,
        });
        proposal_id += 1;
        tasks.push(TechnicalTask {
            id: proposal_id,
            description: format!("Crear tests per validar: {}", intent),
            status: TaskStatus::Proposal,
        });
    }

    Ok(IntentProposal {
        proposal_id: 1,
        original_intent: intent.into(),
        tasks: tasks.clone(),
        explanation: format!(
            "S'han generat {} tasques tècniques basades en la intenció: {}",
            tasks.len(),
            intent
        ),
    })
}

/// Aprova una proposta i afegeix les tasques al context.
pub fn approve_proposal(
    proposal: &IntentProposal,
    context_path: &Path,
) -> Result<ApproveResponse, String> {
    use crate::todo_context::{load_context, save_context, Task};

    // Carregar context actual
    let mut context = load_context(context_path)
        .map_err(|e| format!("Error carregant context: {e}"))?;

    // Convertir tasques de proposta a tasques del context
    let mut tasks_added = 0u32;
    for task in &proposal.tasks {
        context.tasks.push(Task {
            id: task.id + context.tasks.iter().map(|t| t.id).max().unwrap_or(0),
            description: task.description.clone(),
            status: crate::todo_context::TaskStatus::Pending,
        });
        tasks_added += 1;
    }

    // Guardar context actualitzat
    save_context(context_path, &context)
        .map_err(|e| format!("Error guardant context: {e}"))?;

    Ok(ApproveResponse {
        status: "success".into(),
        message: format!(
            "Proposta #{} aprovada. {} tasques afegides.",
            proposal.proposal_id, tasks_added
        ),
        tasks_added,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::todo_context;

    #[test]
    fn test_generate_mock_proposal_greeting() {
        let proposal = generate_mock_proposal("Vull que el sistema saludi en català", "").unwrap();
        assert_eq!(proposal.proposal_id, 1);
        assert!(!proposal.tasks.is_empty());
        assert_eq!(proposal.tasks[0].status, TaskStatus::Proposal);
    }

    #[test]
    fn test_generate_mock_proposal_api() {
        let proposal = generate_mock_proposal("Vull crear un endpoint API", "").unwrap();
        assert_eq!(proposal.proposal_id, 1);
        assert!(proposal.tasks.len() >= 2);
    }

    #[test]
    fn test_generate_mock_proposal_generic() {
        let proposal = generate_mock_proposal("Vull afegir una funcionalitat nova", "").unwrap();
        assert_eq!(proposal.proposal_id, 1);
        assert!(proposal.tasks.len() >= 2);
    }

    #[test]
    fn test_approve_proposal() {
        let tmp = std::env::temp_dir().join("aether_test_approve");
        std::fs::create_dir_all(&tmp).unwrap();

        let context_path = tmp.join("todo-context.json");
        let initial_context = r#"{
            "project_name": "Test",
            "current_stage": "Analyst",
            "tasks": []
        }"#;
        std::fs::write(&context_path, initial_context).unwrap();

        let proposal = IntentProposal {
            proposal_id: 1,
            original_intent: "Test intent".into(),
            tasks: vec![
                TechnicalTask {
                    id: 1,
                    description: "Task 1".into(),
                    status: TaskStatus::Proposal,
                },
                TechnicalTask {
                    id: 2,
                    description: "Task 2".into(),
                    status: TaskStatus::Proposal,
                },
            ],
            explanation: "Test explanation".into(),
        };

        let result = approve_proposal(&proposal, &context_path);
        assert!(result.is_ok(), "Aprovar proposta hauria de funcionar");

        let result = result.unwrap();
        assert_eq!(result.tasks_added, 2);

        // Verificar que les tasques s'han afegit
        let loaded = todo_context::load_context(&context_path).unwrap();
        assert_eq!(loaded.tasks.len(), 2);

        // Netejar
        let _ = std::fs::remove_file(context_path);
        let _ = std::fs::remove_dir(tmp);
    }
}
