# Movies on Other Screens

Aplicación de escritorio **nativa de Linux** para reproducir el mismo vídeo de
forma **simultánea y sincronizada** en varios monitores, usando **Wayland**,
**GTK 4**, **libadwaita** y **libmpv**.

Diseñada siguiendo las convenciones visuales y de interacción de las
aplicaciones modernas de **GNOME**. Software libre bajo **GPL-3.0**.

---

## ¿Qué hace?

- La ventana principal (en el monitor principal) actúa como interfaz de
  control: lista de vídeos, controles de reproducción y selección de
  monitores.
- Los **monitores adicionales** se muestran como **destinos de reproducción**
  seleccionables (uno o varios).
- Los monitores seleccionados reproducen la **misma posición temporal** del
  mismo vídeo.
- Existe **una única reproducción lógica**: una sola decodificación y un único
  flujo de audio (nunca se duplica el audio).

## Requisitos

- **Linux** obligatorio.
- **Wayland** obligatorio (GNOME recomendado). Bajo X11 la aplicación muestra
  un mensaje claro y **sale de forma segura**, sin ofrecer compatibilidad
  parcial.
- Para compilar la interfaz se necesita el **SDK GNOME** (GTK 4, libadwaita) y
  **libmpv**.

## Dependencias

| Necesidad            | Librería / kit          |
|----------------------|-------------------------|
| Lenguaje             | Rust                     |
| Interfaz             | GTK 4 + libadwaita       |
| Reproducción         | libmpv (FFmpeg vía mpv)  |
| Build                | Cargo                    |
| Distribución         | Flatpak                  |

No se implementa ningún codec ni motor de reproducción propio: todo se delega
en **libmpv**.

## Compilar

Compilación de la **lógica de dominio** (sin necesidad de GTK/libmpv):

```sh
cargo test --workspace --no-default-features
```

> La lógica de dominio (`core`) es Rust puro y no depende de GTK; permite
> ejecutar los tests en cualquier máquina con Rust.

La **aplicación completa** (interfaz + reproductor) requiere el SDK GNOME.
Lo más sencillo es construirla **dentro del SDK GNOME de Flatpak** (ver
abajo) o mediante el **CI de GitHub Actions**, que ya dispone de todas las
dependencias y un toolchain de Rust actualizado.

## Ejecutar

Tras construir con Flatpak:

```sh
flatpak run io.github.davichus1303.MoviesOnOtherScreens
```

O directamente el binario:

```sh
./target/release/movies-on-other-screens
```

## Ejecutar tests

```sh
cargo test --workspace --no-default-features
```

Los tests cubren: detección y selección de monitores y navegación entre
vídeos.

## Construir Flatpak

Manifest: `build-aux/io.github.davichus1303.MoviesOnOtherScreens.json`.

```sh
flatpak-builder --user --install build-flatpak \
  build-aux/io.github.davichus1303.MoviesOnOtherScreens.json
flatpak run io.github.davichus1303.MoviesOnOtherScreens
```

El SDK GNOME y la extensión de Rust se descargan automáticamente. El CI de
GitHub Actions (`build-flatpak`) valida la construcción del bundle.

## Arquitectura

El proyecto es un **workspace Cargo** con dos crates:

```text
└── Cargo.toml        (workspace)
    ├── core/         (lógica de dominio, Rust puro, sin GTK)
    │   ├── monitors.rs      # detección/selección de monitores
    │   └── video_list.rs    # vídeos + navegación
    └── app/          (interfaz GTK4 + reproductor libmpv)
        ├── main.rs          # arranque + verificación de Wayland
        ├── wayland.rs       # detección X11/Wayland
        ├── app.rs           # construcción de la ventana
        └── player/          # reproductor lógico único sobre libmpv
```

Principio rector: **separar** la interfaz, la gestión de vídeos y los
monitores. La UI no contiene lógica de negocio compleja; la reproducción no
depende de widgets de GTK.

## Limitaciones conocidas

- **Multi-monitor en pantalla completa (múltiples ventanas)** : la API de
  libmpv (`libmpv/render.h`) documenta que *como máximo existe 1
  `mpv_render_context` por núcleo* (representa la salida de vídeo principal).
  Por tanto **libmpv no soporta** duplicar una misma reproducción a varios
  monitores desde una sola decodificación. Esta app mantiene la reproducción
  **lógica única** (un solo decode, un solo audio) y muestra la salida en el
  monitor principal. La duplicación exacta a N monitores sincronizados es una
  extensión natural futura (composición GL con relectura de frames, estilo
  `mpvpaper`), pero queda fuera del alcance de la primera versión por no estar
  soportada por libmpv.

- **Transición de ~3 s entre vídeos**: libmpv no ofrece un fundido cruzado
  nativo entre dos vídeos de la secuencia. Se aproxima con un fundido de
  entrada de vídeo y audio (~3 s) al cargar el nuevo elemento, evitando cortes
  bruscos. Un fundido cruzado real de dos flujos solapados requeriría
  `lavfi-complex` y queda registrado como mejora.

## Licencia

**GPL-3.0-or-later**. Ver [LICENSE](LICENSE).

El crate `mpv` se distribuye bajo MIT/Apache-2.0; la API cliente de libmpv
bajo ISC. Consulta cada proyecto para sus términos.
