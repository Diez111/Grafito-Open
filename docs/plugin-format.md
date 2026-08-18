# Formato de plugins de Grafito (grafito-plugin.toml)

Un plugin es un directorio con un manifiesto `grafito-plugin.toml`. En v1 es
**declarativo**: no carga binarios ni define handlers dinámicos; activa
capacidades ya incorporadas. La carga es segura (validación fail-closed).

## Ubicación

- Directorio del usuario: `./plugins` (o `$GRAFITO_PLUGINS_DIR`); la app también carga
  los plugins del sistema (`/usr/share/grafito/plugins`) con `PluginRegistry::load_many`
  — el directorio del usuario tiene prioridad si repite un id.
- La UI del asistente (Configuración -> Plugins) lista y activa/desactiva.

## Secciones

```toml
[plugin]
id = "utn.calculo-i"
name = "Cálculo I UTN"
version = "1.0.0"
category = "pedagogy"   # pedagogy | skills | tools | commands | engine
description = "Lecciones de cálculo"
activation = "auto"    # auto | manual
min_app_version = "1.2.20"

[instructions]          # skill pack inyectado al system prompt (acotado)
files = ["intro.md"]
budget_bytes = 2048

[[tools]]               # habilita una tool incorporada
id = "evaluate_expr"

[[commands]]            # comando verificado que el plugin enseña
id = "TangentAt"

[[scenes]]              # plantilla escénica conocida por el motor
template = "derivative-slope"

[engine]                # motor externo invocable por IPC (Fase 4)
transport = "stdio"
command = ["python3", "-u", "-m", "manim_engine"]
protocol_version = 1
capabilities = ["derivative-slope"]
```

## Validación (fail-closed)

- id en minúsculas con separadores seguros y sin rutas.
- Categoría dentro del conjunto admitido; versión semver de 3 componentes.
- `[[tools]]`, `[[commands]]` y `[[scenes]]` deben resolver a capacidades reales;
  si no, el plugin queda en estado «error» y no se activa (ni se puede activar).
- Instrucciones: archivos planos (sin `/`, `..`) y presupuesto acotado a 16 KiB.
- `[engine]`: solo transporte `stdio`, argv acotado, versión de protocolo en rango.

## Estado de activación

`auto` se activa al cargar (salvo que el usuario lo desactive); `manual` exige el
toggle del usuario. La preferencia se persiste en `grafito_config.json`
(`enabled_plugins` / `disabled_plugins`).
