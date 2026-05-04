
"Vull que siguis l'Arquitecte d'un nou sistema anomenat **Aether Code**. És un orquestrador de desenvolupament TDD gamificat escrit en **Rust** i controlat des del mòbil. Pots llegir definition.md i golden-rules.md per entendre la visió general i les regles d'or que has de seguir. 

**La teva primera missió**: Generar l'estructura inicial del projecte (un Cargo Workspace) i el codi del 'Compiler-Agent'.

**Regles estrictes de disseny:**

1. **Llenguatge:** Rust (estricte, usant tokio per asincronia).

2. **Arquitectura:** Un workspace amb dos membres: orchestrator (el cervell) i sandbox (on s'escriurà el codi generat).

3. **Funcionalitat mínima:** L'orquestrador ha de poder rebre una cadena de text (codi Rust), escriure-la a sandbox/src/main.rs i executar cargo check dins de la carpeta sandbox.

4. **Output:** El programa ha de retornar un JSON amb el resultat: { "status": "success/error", "message": "..." }.

Genera el Cargo.toml del workspace, el Cargo.toml de l'orquestrador i el main.rs bàsic de l'orquestrador que faci aquesta gestió de fitxers i comanda."