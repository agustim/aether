Regles d'Or per a un Sistema de Programació Assistida per IA

1. La Regla del "Test Primer" (Strict TDD)
L'eina té prohibit escriure codi d'aplicació si no hi ha un test que falli prèviament.

Flux: L'usuari aprova una Regla de Negoci → L'IA genera el Test → El sistema confirma que el Test falla (Vermell) → L'IA escriu el codi → El Test passa (Verd).

Per què?: Al mòbil no llegiràs 500 línies de Rust, però sí que pots veure un check verd que diu "Càlcul d'IVA: OK".

2. La Regla de l'Arquitectura Immutable
L'IA no pot canviar l'estructura del projecte (folders, crates, dependències principals) sense passar per la pestanya d'Arquitecte i rebre la teva aprovació explícita.

Per què?: Evita que l'IA decideixi pel seu compte canviar de base de dades o de llibreria a mig projecte, cosa que destrossaria el context.

3. La Regla del Context Atòmic (Todo-Context)
El sistema ha de mantenir un fitxer (per exemple, context.json o todo.md) que s'actualitza després de cada acció. Aquest fitxer és l'únic que es passa a l'IA en cada iteració.

Per què?: Perquè les IAs tenen "memòria de peix". Si li passes tot el codi cada vegada, es perd. Si li passes el "Todo-Context", sap exactament on es va quedar.

4. La Regla de la Validació de Compilació (Rust Strictness)
Cap peça de codi es considera "acabada" si no passa el cargo check i el cargo clippy (linter de Rust).

Per què?: Rust és molt estricte. Si el codi compila i passa els tests, la probabilitat que funcioni a producció és altíssima. Això et dóna la seguretat que necessites des del mòbil.

5. La Regla de la Interfície d'Intencions
L'usuari mai interactua amb fitxers individuals, sinó amb Intencions.

Exemple: En lloc de dir "Obre src/main.rs i afegeix un print", dius "Vull que el sistema saludi a l'usuari en entrar". L'orquestrador s'encarrega de saber on va cada cosa.

Per què?: Això permet que l'IA organitzi el codi de la manera més eficient, sense que l'usuari hagi de preocupar-se per detalls d'implementació. A més, facilita la comunicació i evita confusions sobre on va cada cosa.

6. La Regla de l'Historial Atòmic:
Cap canvi es dona per finalitzat si no s'ha fet un git commit. Cada commit ha de portar un missatge estandarditzat generat per l'orquestrador que inclogui:

* La Regla de Negoci implementada.

* El resultat dels tests.

* L'estat del "Todo-Context".

Per què?: Això permet tenir un historial clar i detallat de com s'ha desenvolupat el projecte, facilitant la revisió i la depuració. També serveix com a documentació viva del procés de desenvolupament.