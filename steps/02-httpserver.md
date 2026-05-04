Vull que l'Orquestrador deixi de dependre de stdin i s'exposi al món.

1. Regla de Negoci: L'usuari ha de poder enviar el codi mitjançant una petició HTTP POST.

2. TDD: Primer, genera un test a l'orquestrador que intenti fer un POST a localhost:3000/compile amb un JSON de codi i esperi el nostre JSON de resposta.

3. Implementació: Utilitza axum o actix-web per aixecar el servei.

4. Actualitza el todo.md: Marca el Compiler-Agent com a 'Core' i afegeix 'Interfície API' com a tasca en curs.
