# Multi-Workspace & Permission System
## Objectiu
Transformar l'Orquestrador en un sistema multi-projecte capaç de gestionar diversos entorns de treball (workspaces) de forma aïllada i segura, implementant un control d'accés bàsic per usuari.

## Context
Fins ara, l'Orquestrador treballava en un directori fix. Per escalar, necessitem que pugui gestionar múltiples repositoris i que cada un tingui el seu propi context i permisos.

## Disseny Tècnic
1. Estructura de Directori Dinàmica
L'orquestrador utilitzarà un directori base (/storage) on es crearan subcarpetes per cada workspace_id.

 * Cada workspace tindrà el seu propi todo-context.json.

 * El Sandbox de Docker muntarà com a volum només la carpeta del workspace corresponent.

2. Model de Permisos (ACL)
S'implementarà un fitxer de configuració global o una petita DB per mapejar usuaris i permisos:

Registry: workspaces.json que contingui:

```json
{
  "workspace_id": "factorial-lab",
  "owner": "agusti_id",
  "allowed_users": ["user_1", "user_2"],
  "path": "./storage/factorial-lab"
}
```
3. Seguretat d'Execució
 * Aïllament de xarxa: Cada workspace tindrà una xarxa virtual de Docker aïllada.

 * Quota de recursos: Limitació de memòria (ex: 512MB) i CPU (ex: 0.5 cores) per evitar que un workspace bloquegi el sistema.

## Tasques Tècniques
 * [ ] Crear el WorkspaceManager en Rust per gestionar la creació i validació de rutes.

 * [ ] Modificar els endpoints de l'API per acceptar el header X-Workspace-ID i X-User-ID.

 * [ ] Implementar middleware de validació: user té permís sobre workspace?