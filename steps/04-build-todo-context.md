Vull que l'Orquestrador gestioni l'estat del projecte mitjançant un fitxer todo-context.json situat a l'arrel del workspace.

1. Estructura del JSON:
El fitxer ha de tenir camps per a: project_name, current_stage (Arquitecte/Analista/QA/Dev), i una llista de tasks amb id, description i status (pending/in_progress/completed).

2. Nous Endpoints HTTP:

GET /context: Retorna el JSON actual.

POST /context/task: Per afegir una nova tasca des del mòbil.

3. Lògica de Transició d'Estat:
Modifica el handler de /compile perquè, en cas de success, busqui la tasca que estigui en in_progress i la marqui com a completed. Després, ha de fer un git commit que inclogui el canvi al todo-context.json.

4. TDD:
Crea un test d'integració que:

     1. Inicialitzi un context amb una tasca 'pendent'.

     2. La passi a 'in_progress'.

     3. Envii un codi vàlid a /compile.

     4. Verifiqui que el context final té la tasca com a 'completed'.

Regla d'Or 3: El fitxer todo-context.json és l'única font de veritat per a l'estat del joc.