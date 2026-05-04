# El Bucle de Codificació (Self-Correction Loop)
## Objectiu
Implementar la lògica perquè l'Orquestrador no només enviï codi al Sandbox, sinó que sigui capaç de llegir els errors de compilació, enviar-los de tornada al LLM i intentar corregir el codi automàticament fins a aconseguir una execució reeixida.

Context
Ara que tenim el LLMClient (Pas 07) i el Sandbox (Pas 04/05) funcionant, hem d'unir-los. Si l'Agent genera un factorial però oblida un punt i coma, el sistema ha de detectar l'error i arreglar-ho sense intervenció humana.

## Requisits Tècnics
1. El Mòdul coding_agent.rs
Cal crear o ampliar l'agent per gestionar el flux de reintents:

 * Estructura de control: Un bucle while o recursiu limitat (màxim 3-5 intents) per evitar bucles infinits i consum excessiu de tokens.

 * Extracció de codi: Un parser que netegi la resposta del LLM (sovint l'IA retorna text + blocs de codi ```rust). L'agent ha d'extreure només el contingut del bloc de codi.

2. Prompt de Correcció (The Fixer Prompt)
Quan el Sandbox retorna un exit_status != 0, l'Orquestrador ha de generar un nou prompt per al LLM:

 * Input: Codi anterior + Missatge d'error del compilador (stderr).

 * Instrucció: "El codi anterior ha fallat amb aquest error. Si us plau, corregeix-lo i retorna el fitxer complet actualitzat."

3. Gestió d'Estat a todo-context.json
 * Mentre l'agent està treballant, la tasca ha d'estar en in_progress.

 * Si després dels intents màxims el codi encara falla, la tasca passa a failed amb un log de l'error final.

 * Si el sandbox retorna success, la tasca passa a completed.

## Flux de Treball (The Loop)
 1. Trigger: L'usuari o el sistema activa una tasca de tipus "Implementació".

 2. Generació: El LLM genera una primera versió del codi.

 3. Sandbox: S'executa el codi en el contenidor Docker.

 4. Validació:

 * Èxit: Es guarda el codi final, es fa commit i es tanca la tasca.

 * Error: Es recull el stderr, s'envia al LLM i es torna al punt 2.

## Definició de "Done" (Tests d'acceptació)
* [ ] Test de Bucle: Forçar un error de sintaxi manualment en un test i verificar que l'agent fa almenys un reintent amb el missatge d'error.

* [ ] Test de Persistència: El codi final corregit s'ha de guardar correctament en el sistema de fitxers (o el log del commit).

* [ ] Test de Seguretat: Verificar que durant el bucle, el sandbox manté les restriccions d'usuari aether i no té xarxa.