Vull que l'Orquestrador validi el codi dins d'un contenidor Docker.

1. Configuració del Docker:

    * Utilitza una imatge base de Rust oficial (rust:1.75-slim).

    * Confi*gura el contenidor per funcionar sense xarxa (network_mode: none).

    * Defineix un timeout de 60 segons: si el procés no acaba, es mata el contenidor (protecció contra bucles infinits).

2. Gestió de Dependències (Offline-first):

    * L'orquestrador ha d'assegurar-se que les dependències estan descarregades a l'hoste abans de llançar el Docker.

    * Munta la memòria cau de Cargo (~/.cargo/registry) i la carpeta target del sandbox com a volums en el contenidor.

3. Accions de l'Orquestrador:

    * El mètode check_code ara ha d'invocar docker run.

    * Ha d'executar cargo check i cargo test --lib (per validar les regles de negoci).

4. TDD i Seguretat:

    * Crea un test que intenti fer un ping o una petició HTTP des del codi generat. El test ha de fallar o retornar un error de xarxa per confirmar que el sandbox és estanc.

Actualització del context: Un cop implementat, marca la tasca 'Seguretat de Sandbox' com a completed al todo-context.json.