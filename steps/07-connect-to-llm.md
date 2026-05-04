## Objectiu
Dotar l'Orquestrador de capacitat de raonament connectant-lo a un model de llenguatge (LLM) mitjançant una interfície compatible amb l'API d'OpenAI. Això permetrà passar d'un sistema basat en "mocks" a un sistema que genera propostes i codi real.

## Context
Fins ara, l'Orquestrador responia a /intent amb propostes estàtiques. Ara, utilitzarà el text de l'usuari i l'estat del todo-context.json per consultar a un LLM extern.

Requisits Tècnics
1. Variables d'Entorn
S'ha d'implementar suport per a un fitxer .env (o variables d'entorn globals) amb els següents camps  (Genera un fitxer env.example amb aquests camps i instruccions clares de com omplir-los):

 * AETHER_LLM_URL: La URL base de l'API (ex: https://elmeullm/v1).

 * AETHER_LLM_KEY: La clau d'autenticació (Bearer Token).

 * AETHER_LLM_MODEL: El nom del model a utilitzar (ex: gpt-4, qwen-2.5-coder).


2. Client de Rust (orchestrator/src/llm_client.rs)
Utilitzar reqwest per a les crides HTTP asíncrones.

* Implementar una estructura LLMClient que gestioni els headers i el format JSON.

* Contracte d'entrada: System Prompt + User Prompt.

* Contracte de sortida: String amb la resposta (habitualment JSON generat per l'IA).

3. Prompting Strategy
S'han de definir dos prompts base:

* System Prompt: Defineix l'Agent com un "Expert en Rust i Arquitectura de Sistemes" que només respon en format JSON per facilitar el parsing.

Context Prompt: Una funció que injecti el contingut de todo-context.json a la consulta perquè l'IA sàpiga quines tasques estan pendents.

## Canvis en els Endpoints Existents
POST /intent
1. Rebrà el JSON: {"intent": "missatge de l'usuari"}.

2. Llegirà el todo-context.json.

3. Enviarà una petició al LLM.

4. El LLM retornarà una IntentProposal (JSON).

5. L'Orquestrador guardarà aquesta proposta a proposals.json i retornarà el seu ID.

## Flux de dades
1. User -> POST /intent -> Orchestrator

2. Orchestrator -> LLM (v1/chat/completions) -> Orchestrator

3. Orchestrator -> proposals.json

4. Orchestrator -> User (Proposta per revisar)

Tests d'acceptació
[ ] Test de Connectivitat: Un test d'integració que faci una crida real (o amb un servidor mock tipus wiremock) per verificar que els headers d'autorització i la URL són corrects.

[ ] Test de Parsing: Verificar que si l'IA retorna un JSON mal format, l'orquestrador ho gestiona sense petar.

[ ] Test d'Entorn: Confirmar que si falten les variables d'entorn, el sistema dona un error explicatiu en lloc de fer un "panic".

Recorda hi ha un grup de regles d'or a golden-rules.md que has de seguir en tot moment.
Si detectes alguna fisura en aquestes regles, has de reportar-ho immediatament i proposar una solució.