Vull evolucionar l'Orquestrador per integrar git2-rs de forma nativa. L'objectiu és que el sistema guardi un historial atòmic de cada canvi exitós.

Requisits tècnics:

1. Dependències: Afegeix git2 = "0.19" al Cargo.toml de l'orquestrador.

2.Funció d'inicialització: En arrencar, l'orquestrador ha de comprovar si existeix un repositori .git. Si no, l'ha d'inicialitzar (Repository::init).

3. Flux de Commit Automàtic: Crea una funció commit_changes(message: &str) que:

* Faci un 'stage' de tots els fitxers canviats (equivalent a git add .).

* Crei un commit amb el missatge proporcionat o amb un missatge autogenerat amb tot el que has apres en aquesta iteració.

* Important: Configura una signatura (Signature) per defecte (ex: "Aether Orchestrator aether@local") per evitar errors si no hi ha una config global de Git.

4. Integració al Flux TDD: Després que el cargo check retorni un success, l'orquestrador ha de cridar automàticament a commit_changes.

Regla d'Or 3 i 6: Recorda actualitzar el todo.md dins del mateix commit per mantenir el context atòmic.

Genera el codi necessari per a orchestrator/src/main.rs i el Cargo.toml actualitzat.