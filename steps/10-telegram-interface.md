# Interfície de Telegram (The Bot Bridge)
## Objectiu
Substituir (o complementar) les crides curl per una interfície de xat asíncrona que permeti interactuar amb l'Orquestrador des de qualsevol lloc, facilitant el flux d'aprovació i supervisió.

## Context
L'ús de Telegram permet rebre notificacions quan l'Agent de Codificació (Pas 08) acaba una tasca o requereix intervenció humana, fent el procés molt més natural.

## Funcionalitats del Bot
1. Comandes Principals
 * ```/new_intent <text>``: Envia una intenció al workspace actiu.
 * ```/status```: Mostra el resum visual del todo-context.json (Tasques completades/pendents).
 * ```/switch <workspace_id>```: Canvia el context de treball de l'usuari.

2. Flux d'Aprovació amb Botons (Inline Keyboards)
Quan l'IA genera una proposta (Pas 07), el bot enviarà un missatge amb:

 * El text de la proposta.

 * Dos botons: [✅ Aprovar] i [❌ Rebutjar].
L'aprovació dispararà automàticament el mètode context/approve de l'Orquestrador.

3. Mode Streaming de Logs
 * ```/logs```: Mostra les últimes línies del stderr del sandbox si una tasca ha fallat.

 * Notificació automàtica quan el Correction Loop té èxit després de X intents.

## Requisits Tècnics
* Biblioteca: teloxide per a Rust (framework asíncron).

 * Seguretat: El bot només respondrà a IDs de Telegram autoritzats a workspaces.json. I utilitzarà un token definit a .env per connectar-se a l'API de Telegram. (recorda afegir-ho a env.example)

 * Persistència: Guardar el chat_id per poder enviar notificacions "push" sense que l'usuari pregunti.