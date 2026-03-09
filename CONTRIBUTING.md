# Contribuir a hitt

## Requisitos

- **Rust** 1.70+ (edición 2021)
- **cargo** (incluido con Rust)

## Setup rápido

```bash
git clone <repo> && cd hitt
cargo build             # Compilar
cargo test              # ~285 tests
cargo clippy            # Pedantic, cero warnings
cargo run               # Modo TUI (interactivo)
cargo run -- send GET https://httpbin.org/get   # Modo CLI
```

## Estructura del proyecto

```
src/
├── main.rs                  # Punto de entrada, bifurcación TUI/CLI
├── app.rs                   # Estado central (App, AppMode, FocusArea)
├── lib.rs                   # Exports del crate
├── cli.rs                   # Modo CLI (subcomandos)
│
├── core/                    # Modelo de dominio
│   ├── request.rs           #   Request, Protocol, HttpMethod, RequestBody
│   ├── response.rs          #   Response, Timing, Cookies
│   ├── collection.rs        #   Collection, CollectionItem (árbol recursivo)
│   ├── environment.rs       #   Environment, variables
│   ├── variables.rs         #   VariableResolver (cadena de scopes)
│   ├── client.rs            #   HttpClient (reqwest wrapper)
│   ├── chain.rs             #   RequestChain, ChainStep, ValueExtraction
│   ├── chain_executor.rs    #   Ejecución de chains
│   ├── auth.rs              #   Bearer, Basic, ApiKey, OAuth2
│   ├── cookie_jar.rs        #   CookieJar
│   ├── history.rs           #   HistoryStore, HistoryEntry
│   ├── error.rs             #   HittError (thiserror)
│   ├── helpers.rs           #   Utilidades
│   └── constants.rs         #   Constantes
│
├── event/                   # Sistema de eventos
│   ├── mod.rs               #   AppEvent, EventHandler, handle_event()
│   ├── key_handlers.rs      #   Normal mode (vim keybindings)
│   ├── insert_mode.rs       #   Insert mode (edición de texto)
│   ├── command_mode.rs      #   Command mode (:comandos)
│   ├── commands.rs          #   30+ comandos de la paleta
│   ├── modal_handlers.rs    #   Modales (search, help, import, export)
│   ├── mouse_handlers.rs    #   Click y scroll con hit-testing
│   ├── navigation.rs        #   Navegación direccional
│   ├── sidebar.rs           #   Estado del sidebar
│   ├── chain.rs             #   Ejecución de chains
│   ├── protocols.rs         #   Eventos WS/SSE
│   ├── persistence.rs       #   Guardar a disco
│   └── import_export.rs     #   Import/export con auto-detección
│
├── ui/                      # Interfaz de usuario
│   ├── layout.rs            #   render() principal, composición de UI
│   ├── theme.rs             #   Temas (catppuccin, dracula, gruvbox, tokyo-night)
│   └── widgets/             #   Widgets individuales
│       ├── request_panel.rs #     Panel de request
│       ├── response_panel.rs#     Panel de response
│       ├── assertion_panel.rs#    Resultados de aserciones
│       ├── sidebar.rs       #     Sidebar (collections/chains/history)
│       ├── diff_viewer.rs   #     Visor de diff side-by-side
│       ├── tab_bar.rs       #     Barra de tabs
│       ├── status_bar.rs    #     Barra de estado
│       ├── search_modal.rs  #     Modal de búsqueda fuzzy
│       ├── help_modal.rs    #     Modal de ayuda
│       └── env_selector.rs  #     Selector de environment
│
├── protocols/               # Protocolos en tiempo real
│   ├── websocket.rs         #   WebSocket (tokio-tungstenite)
│   ├── sse.rs               #   Server-Sent Events
│   └── grpc.rs              #   gRPC (tonic)
│
├── importers/               # Importadores
│   ├── curl.rs              #   cURL → Request
│   ├── openapi.rs           #   OpenAPI 3.x → Collection
│   ├── har.rs               #   HAR → Collection
│   ├── chain.rs             #   YAML → RequestChain
│   └── dotenv.rs            #   .env → HashMap
│
├── exporters/               # Exportadores
│   ├── curl.rs              #   Request → cURL
│   └── markdown_docs.rs     #   Collection → Markdown
│
├── postman/                 # Interop con Postman
│   ├── import.rs            #   Postman Collection → Collection
│   ├── export.rs            #   Collection → Postman Collection
│   ├── env_import.rs        #   Postman Environment → Environment
│   ├── env_export.rs        #   Environment → Postman Environment
│   └── schema_v2_1.rs       #   Structs del schema Postman v2.1
│
├── storage/                 # Persistencia
│   ├── config.rs            #   AppConfig (TOML)
│   ├── collections_store.rs #   Colecciones (JSON files)
│   └── history_db.rs        #   Historial (sled)
│
├── testing/                 # Testing y aserciones
│   ├── assertion_engine.rs  #   12 tipos de aserción
│   ├── diff.rs              #   Diff de responses
│   ├── load_test.rs         #   Pruebas de carga
│   └── schema_validator.rs  #   Validación JSON Schema
│
├── proxy/                   # Proxy inspector
│   ├── server.rs            #   ProxyServer
│   ├── capture.rs           #   CapturedRequest, CaptureStore
│   └── filter.rs            #   ProxyFilter
│
└── utils/                   # Utilidades
    ├── pretty_print.rs      #   Formateo de JSON
    ├── clipboard.rs         #   Acceso al portapapeles
    └── timing.rs            #   Medición de tiempos
```

Para más detalle, ver [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Convenciones

### Modelos de datos

- Todos los modelos usan `#[derive(Serialize, Deserialize)]` de serde
- IDs son `Uuid` (v4) generados con `Uuid::new_v4()`
- Timestamps son `DateTime<Utc>` de chrono
- Colecciones tienen estructura de árbol recursivo (`CollectionItem::Folder` contiene `Vec<CollectionItem>`)

### Manejo de errores

- **Errores de dominio**: `thiserror` con el enum `HittError` en `src/core/error.rs`
- **Errores de aplicación**: `anyhow::Result` para propagación rápida
- Pattern: funciones de core retornan `Result<T, HittError>`, handlers retornan `anyhow::Result<()>`

### Widgets UI

- **Stateless**: Los widgets leen de `&App` y renderizan en `Frame`. No almacenan estado propio.
- **Patrón**: Implementan `ratatui::widgets::Widget` trait o son funciones `fn render(app: &App, frame: &mut Frame, area: Rect)`
- **ClickableRegions**: Cada widget actualiza `app.regions` con los `Rect` de elementos interactivos

### Tests

- Tests de integración en `tests/` (10 archivos, ~285 tests)
- Fixtures en `tests/fixtures/` (6 archivos de ejemplo)
- Tests async con `#[tokio::test]` para operaciones que requieren runtime
- Helpers comunes: `test_app()`, `make_response()`, `test_config()`
- Directorios temporales con `tempfile::TempDir` para tests de persistencia

---

## Cómo agregar...

### Un nuevo widget

1. Crear `src/ui/widgets/mi_widget.rs`
2. Implementar como función o struct con `Widget` trait:
   ```rust
   pub fn render_mi_widget(app: &App, frame: &mut Frame, area: Rect) {
       // Leer estado de app
       // Crear widget de ratatui
       // frame.render_widget(widget, area);
       // Registrar regiones clickeables si aplica
   }
   ```
3. Registrar en `src/ui/widgets/mod.rs`
4. Llamar desde `src/ui/layout.rs` en la función `render()`

### Un nuevo importador

1. Crear `src/importers/mi_formato.rs`
2. Implementar función de parsing:
   ```rust
   pub fn import_mi_formato(content: &str) -> Result<Collection> {
       // Parsear contenido
       // Convertir a Collection/Request
   }
   ```
3. Registrar en `src/importers/mod.rs`
4. Agregar detección en `src/event/import_export.rs` → `execute_import()`
5. Agregar tests en `tests/importers.rs`
6. Agregar fixture en `tests/fixtures/` si aplica

### Un nuevo comando

1. Agregar el comando en `src/event/commands.rs` → `execute_command()`:
   ```rust
   "micomando" => {
       // Implementación
   }
   ```
2. Si necesita UI (modal, input), agregar variant en `ModalKind`
3. Si necesita soporte CLI, agregar variant en `src/cli.rs` → `Commands` enum
4. Agregar tests en `tests/event_commands.rs`

### Un nuevo protocolo

1. Crear `src/protocols/mi_protocolo.rs`
2. Seguir el **bridge task pattern**:
   ```rust
   // Definir comandos y eventos
   pub enum MiProtocoloCmd { Connect, Disconnect, ... }
   pub enum MiProtocoloEvent { Connected, Disconnected, ... }

   // Función de conexión que retorna sender de comandos
   pub async fn connect(
       url: &str,
       event_tx: mpsc::UnboundedSender<AppEvent>,
   ) -> mpsc::UnboundedSender<MiProtocoloCmd> {
       let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
       tokio::spawn(async move {
           // Loop: escuchar cmd_rx + eventos del protocolo
           // Enviar AppEvent al event_tx
       });
       cmd_tx
   }
   ```
3. Agregar variant en `Protocol` enum (`src/core/request.rs`)
4. Agregar variant en `AppEvent` (`src/event/mod.rs`)
5. Agregar handler en `src/event/protocols.rs`
6. Agregar session en `RequestTab` (`src/app.rs`)
7. Agregar response tabs en `ResponseTabKind` (`src/app.rs`)

---

## Documentación detallada

| Documento | Contenido |
|-----------|-----------|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Visión general, mapa de módulos, flujos, modelo de datos |
| [docs/EVENTOS.md](docs/EVENTOS.md) | Sistema de eventos, handlers, keybindings, paleta de comandos |
| [docs/PROTOCOLOS.md](docs/PROTOCOLOS.md) | WebSocket, SSE, gRPC, bridge task pattern |
| [docs/ALMACENAMIENTO.md](docs/ALMACENAMIENTO.md) | AppConfig, CollectionsStore, HistoryDb |
| [docs/TESTING.md](docs/TESTING.md) | Motor de aserciones, pruebas de carga, suite de tests |
| [docs/IMPORTERS_EXPORTERS.md](docs/IMPORTERS_EXPORTERS.md) | Todos los importadores y exportadores |
