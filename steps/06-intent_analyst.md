Vull que l'Orquestrador sigui capaç de transformar 'Intencions' de l'usuari en 'Tasques' de desenvolupament.

1. Nou Endpoint: POST /intent

L'usuari envia un text lliure (ex: 'Vull que el sistema saludi en català').

2. Lògica de l'Analista:

    * L'Orquestrador ha d'enviar aquesta intenció a l'IA (Qwen) juntament amb el todo-context.json actual.

    * L'IA ha de retornar una llista de mini-tasques tècniques (ex: 1. Crear test de salutació, 2. Implementar funció salutació).

3. Interacció Mòbil (Feedback):

L'Orquestrador no afegeix les tasques directament. Primer les retorna com a 'Proposta'.

L'usuari ha de fer un POST /context/approve per confirmar que vol aquestes tasques al seu full de ruta.

4. TDD:

Crea un test on s'enviï una intenció, es rebi la proposta de l'IA i es verifiqui que el format és correcte per al todo-context.json.


---

Recorda hi ha un grup de regles d'or a golden-rules.md que has de seguir en tot moment.
Si detectes alguna fisura en aquestes regles, has de reportar-ho immediatament i proposar una solució.