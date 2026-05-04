🛰️ Projecte: "Aether Code" (Nom suggerit)
L'IDE Remot Gamificat i Auto-Evolutiu per a Mòbil.

1. Visió General
Aether és un entorn de desenvolupament "headless" basat en Rust i controlat mitjançant una interfície mòbil optimitzada per a polzes. El sistema delega l'escriptura de codi a models de llenguatge avançats (com Qwen Code, etc.) sota un paradigma de control estrictament basat en TDD (Test-Driven Development) i regles de negoci. Segueix el document golden-rules.md com a guia per a les regles d'or que asseguren la qualitat, la seguretat i la coherència del codi generat.

L'objectiu final és un sistema bootstrapped: una eina que l'usuari utilitza per programar les mateixes millores de l'eina, expandint les seves capacitats sessió rere sessió.

2. Conceptes Clau
* L'Orquestrador (The Game Master): Un servei en Rust que gestiona el context, coordina les crides a l'IA i assegura que cap codi s'integri si no passa els tests i la compilació.

* Pestanyes de Rol (Modular Flow):

  * Arquitecte: Disseny de l'esquelet i el stack.

  * Analista: Definició de regles de negoci (lògica pura).

  * QA Engineer: Generació automàtica de tests basats en les regles.

  * Dev (Qwen): Implementació de codi per passar els tests.

* Todo-Context: Un document viu que actua com a memòria compartida. Conté l'estat actual dels tests (🔴/🟢) i la llista de tasques pendents.

3. El Bucle de Joc (Gameplay Loop)

* Definició d'Intencions: L'usuari proposa una funcionalitat des del mòbil.

* Checkpoint de Seguretat: L'Orquestrador fa preguntes de clarificació fins que la intenció és executable.

* Cicle TDD:

  * L'IA crea un test que falla.

  * L'IA escriu el codi en un Sandbox Docker.

  * El compilador de Rust valida la seguretat i els tests.

* Evolució: Un cop el test és verd, el nou codi es fusiona (merge) amb el nucli de l'eina, desbloquejant noves capacitats.

4. Stack Tecnològic de Referència
* Core: Rust (seguretat, velocitat, multithreading).

* Engine: Qwen 2.5 Coder (via API o local).

* Sandbox: Docker (aïllament total de l'execució).

* Interfície: WebApp Mobile-First / Telegram Bot (interacció basada en blocs).

🛠️ Què fem amb això ara?
Perquè això passi de document a realitat, t'he preparat la primera "Missió de Desenvolupament". Com que hem dit que hem de ser decidits, aquí tens el pla:

Pas 1: Crear el "Contenidor Mare"
Necessitem un repositori de Rust buit (crea'l aquí mateix) amb una estructura de Workspace. Això ens permetrà tenir el codi de l'Orquestrador en una banda i el codi que l'IA vagi generant en una altra, sense que es barregin les dependències.
