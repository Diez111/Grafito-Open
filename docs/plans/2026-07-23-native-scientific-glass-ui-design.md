# Native Scientific Glass UI Design

## Problem
Grafito ya tiene temas y componentes nativos, pero dos configuraciones de estilo compiten entre si y el chrome principal no comunica una jerarquia visual consistente. El resultado es funcional, pero no transmite la calidad de un banco matematico profesional.

## Reframe
No se busca convertir Grafito en una interfaz web ni ocultar controles densos. El corte debe hacer que la misma herramienta nativa se perciba mas clara, tactil y deliberada sin reducir el area de trabajo matematico.

## Approach
Se adopta un corte equilibrado de "scientific glass" nativo: temas semanticos como unica fuente de verdad, superficies levemente translucidas, bordes y sombras de baja intensidad, y transiciones cortas de estado para controles que ya existen. Se descartan una reestructuracion de navegacion y animaciones decorativas persistentes.

## Scope
Incluye la barra superior, toolbar, rail lateral, barra de entrada, tarjetas de Algebra y estilos globales de egui. Incluye crossfades de hover/seleccion, contraste comprobado en ambos temas y un selector de paneles que conserva el acceso al trabajo en modo compacto. Excluye cambios de flujo, nuevas dependencias, fuentes remotas, WebView y cambios en la geometria/canvas.

## Technical Design
`grafito-ui::theme::Theme::apply` define de forma exclusiva rounding, sombras, espaciado, timing y colores globales. Se elimina la pasada de estilo de `grafito-app` que sobrescribia esos valores. Los componentes del chrome interpolan sus superficies entre estados usando el reloj de egui; el movimiento dura 180 ms y solo acompana hover o seleccion. Los estados activos conservan texto, icono y un indicador de forma para no depender solo del color.

## Acceptance Criteria
1. El primer frame y un cambio de tema usan el mismo estilo global, sin una segunda configuracion que altere radios, sombras o espaciado.
2. Paneles, barras y rail usan superficies translucidas con bordes visibles y contraste de texto AA en temas claro y oscuro.
3. Los tabs del rail y grupos de toolbar transicionan entre hover/seleccion sin desplazar el layout.
4. Los controles existentes conservan etiquetas accesibles, foco y sus atajos; el modo compacto mantiene acceso a cada panel de trabajo y no se agregan animaciones continuas salvo indicadores de trabajo ya existentes.
5. Las pruebas de UI cubren los tokens de transparencia, el timing de microinteraccion y la presencia de animaciones de estado en el chrome.

## Test Strategy
Pruebas unitarias de tema verifican alpha, contraste y `animation_time`. Las pruebas de integracion de UI verifican que rail y toolbar usan la animacion nativa de egui. Los gates del workspace validan formato, lints, regresiones y build de release.

## Risks
La transparencia excesiva puede reducir jerarquia sobre el canvas; los alpha se mantienen moderados y los bordes semanticos separan superficies. Las animaciones pueden afectar accesibilidad si son decorativas; se limitan a transiciones de estado cortas y no bloquean la interaccion.
