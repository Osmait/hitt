# Aserciones y pruebas de carga

> Ver también: [ARCHITECTURE.md](./ARCHITECTURE.md) para la visión general.

hitt incluye un motor de aserciones para validar responses y un framework de pruebas de carga integrado.

---

## Motor de aserciones

**Implementación:** `hitt-core/src/testing/assertion_engine.rs`

### Tipos de aserción

hitt soporta 12 tipos de aserción organizados en 5 categorías:

#### Status

| Tipo | Descripción | Ejemplo |
|------|-------------|---------|
| `StatusEquals(u16)` | Status code exacto | `status == 200` |
| `StatusRange(u16, u16)` | Status en rango inclusivo | `200 <= status <= 299` |

#### Body

| Tipo | Descripción | Ejemplo |
|------|-------------|---------|
| `BodyContains(String)` | Body contiene substring | `body contains "success"` |
| `BodyPathEquals { path, expected }` | JSONPath retorna valor esperado | `$.user.name == "John"` |
| `BodyPathExists(String)` | JSONPath existe en response | `$.data.items exists` |
| `BodyPathType { path, expected }` | JSONPath retorna tipo esperado | `$.count is Number` |
| `BodyPathContains { path, substring }` | JSONPath contiene substring | `$.message contains "ok"` |
| `MatchesJsonSchema(Value)` | Body valida contra JSON Schema | Schema validation |

#### Headers

| Tipo | Descripción | Ejemplo |
|------|-------------|---------|
| `HeaderEquals { name, expected }` | Header tiene valor exacto | `Content-Type == "application/json"` |
| `HeaderExists(String)` | Header existe en response | `X-Request-Id exists` |

#### Performance

| Tipo | Descripción | Ejemplo |
|------|-------------|---------|
| `ResponseTimeLessThan(Duration)` | Tiempo de response menor a SLA | `time < 500ms` |
| `SizeLessThan(usize)` | Tamaño de body menor a límite | `size < 1MB` |

### Tipos JSON para BodyPathType

```rust
pub enum JsonType {
    String,
    Number,
    Boolean,
    Array,
    Object,
    Null,
}
```

### API del motor

```rust
pub struct AssertionEngine;

impl AssertionEngine {
    /// Ejecuta todas las aserciones contra una response
    pub fn run_assertions(
        assertions: &[Assertion],
        response: &Response,
    ) -> Vec<AssertionResult>;

    /// Evalúa una sola aserción
    pub fn evaluate(
        assertion: &Assertion,
        response: &Response,
    ) -> AssertionResult;

    /// Retorna (passed, total)
    pub fn summary(results: &[AssertionResult]) -> (usize, usize);
}
```

### Resultado de aserción

```rust
pub struct AssertionResult {
    pub assertion: Assertion,
    pub passed: bool,
    pub actual_value: Option<String>,   // Valor actual para debugging
    pub message: String,                // Mensaje legible del resultado
}
```

### Integración con el flujo

Las aserciones se ejecutan automáticamente después de cada request HTTP:

1. User envía request → `HttpClient::send()` retorna `Response`
2. `AssertionEngine::run_assertions()` ejecuta las aserciones definidas en el request
3. Los `AssertionResult` se almacenan en `response.assertion_results`
4. La UI muestra resultados en el tab "Assertions" con indicadores verde/rojo

---

## JSONPath

**Crate:** `jsonpath-rust` 0.7

Se usa en dos contextos:

### 1. Aserciones

```rust
// Evalúa un JSONPath contra el body de la response
fn eval_jsonpath(response: &Response, path: &str) -> Option<serde_json::Value> {
    let json = response.body_json()?;
    let jsonpath: JsonPath = path.parse().ok()?;
    let result = jsonpath.find(&json);
    // Retorna el primer elemento si es array, sino el valor directo
}
```

Ejemplos de JSONPath:
- `$.user.name` → `"John"`
- `$.data.items[0].id` → `42`
- `$.data.items.length()` → `3`
- `$.store.book[?(@.price < 10)]` → libros baratos

### 2. Extracción en chains

Los `ChainStep` pueden extraer valores de responses para usarlos en pasos posteriores:

```rust
pub struct ValueExtraction {
    pub source: ExtractionSource,
    pub json_path: String,        // JSONPath para body
    pub variable_name: String,    // Nombre de la variable extraída
}

pub enum ExtractionSource {
    Body,              // Extraer de JSON body con JSONPath
    Header(String),    // Extraer valor de header específico
    Cookie(String),    // Extraer valor de cookie
    Status,            // Extraer status code
}
```

### 3. Status HTTP de un paso de chain

Un paso de chain es **exitoso cuando el request se completa**, sea cual sea el status HTTP: un `500` o un `404` sin condición cuenta como éxito (los pasos no ejecutan las assertions de la request; eso lo hace el collection runner / `hitt test`). Un paso solo falla si el request no se pudo enviar (error de red, timeout, request borrada de la colección); entonces la chain se detiene.

- `hitt chain run` incluye el `status` de cada paso en su JSON y sale con código 0 si ningún paso falló, aunque haya status ≥ 400:

  ```json
  { "chain": "Signup", "steps": [ { "step": 1, "request": "create-user", "status": 500, "timing_ms": 12, "extractions": {} } ], "variables": {} }
  ```

  Para tratar un 4xx/5xx como error en CI, revisar `steps[].status` (p. ej. `jq -e 'all(.steps[]; (.status // 0) < 400)'`).
- Para no seguir después de un status de error, poner una `condition` en el paso siguiente (se evalúa contra la response del paso anterior); si no se cumple, el paso queda *skipped*:

  ```yaml
  - request: get-user
    condition:
      type: status_range
      from: 200
      to: 299
  ```

---

## JSON Schema validation

**Crate:** `jsonschema` 0.26
**Implementación:** `hitt-core/src/testing/schema_validator.rs`

```rust
pub fn validate(
    instance: &Value,   // JSON del response body
    schema: &Value,     // JSON Schema definition
) -> Result<(), Vec<String>>
```

Valida el body de la response contra un JSON Schema. Retorna errores detallados con paths si la validación falla.

---

## Diff de responses

**Crate:** `similar` 2.x
**Implementación:** `hitt-core/src/testing/diff.rs`

Permite comparar dos responses línea por línea:

```rust
pub struct DiffResult {
    pub lines: Vec<DiffLine>,
    pub additions: usize,
    pub deletions: usize,
    pub unchanged: usize,
}

pub struct DiffLine {
    pub tag: DiffTag,              // Equal | Insert | Delete
    pub content: String,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
}

pub fn diff_responses(left: &Response, right: &Response) -> DiffResult
pub fn diff_text(left: &str, right: &str) -> DiffResult
```

La UI muestra el diff en un visor side-by-side con líneas coloreadas (verde para adiciones, rojo para eliminaciones).

---

## Pruebas de carga

**Implementación:** `hitt-core/src/testing/load_test.rs`

### Configuración

```rust
pub struct LoadTestConfig {
    pub request_id: Uuid,          // Request a ejecutar
    pub total_requests: u32,       // Total de requests a enviar
    pub concurrency: u32,          // Workers paralelos
    pub ramp_up_seconds: Option<u32>,  // Incremento gradual de carga
    pub timeout_ms: u64,           // Timeout individual
}
```

### Resultados

```rust
pub struct LoadTestResult {
    pub total_requests: u32,
    pub successful: u32,
    pub failed: u32,
    pub error_rate: f64,                           // Porcentaje de errores
    pub duration: Duration,                        // Duración total
    pub rps: f64,                                  // Requests por segundo
    pub latency: LatencyStats,
    pub status_distribution: HashMap<u16, u32>,    // Status codes → conteos
    pub errors: Vec<String>,                       // Mensajes de error
}

pub struct LatencyStats {
    pub min: Duration,
    pub max: Duration,
    pub mean: Duration,
    pub median: Duration,
    pub p90: Duration,    // Percentil 90
    pub p95: Duration,    // Percentil 95
    pub p99: Duration,    // Percentil 99
    pub std_dev: Duration,
}
```

### Uso

```
:loadtest 1000 50     # 1000 requests con 50 workers concurrentes
```

---

## Collection runner

**Implementación:** `hitt-core/src/testing/collection_runner.rs` (TUI: `hitt-tui/src/event/runner.rs` + `ui/widgets/runner.rs`; CLI: `hitt test`)

Envía secuencialmente todas las requests de una colección (o de una carpeta) en orden de árbol (carpetas recursivas), ejecuta las assertions de cada una con `AssertionEngine::run_assertions` y produce un reporte.

- **Variables**: `VariableResolver::from_context(None, &collection.variables, env, None, None)` — el environment activo sobrescribe las variables de colección.
- **Auth heredada** (semántica Postman): una request sin auth o con `AuthConfig::Inherit` usa la auth de la carpeta más cercana que defina una, y si no la de la colección. `AuthConfig::None` = sin auth explícito.
- **Protocolos**: WebSocket / SSE / gRPC se reportan como `skipped`.
- **Estado por request**: `passed` (response + todas las assertions OK; sin assertions cuenta como passed), `failed` (alguna assertion falla), `error` (no se pudo enviar), `skipped`.

```rust
pub struct RunnerOptions {
    pub folder: Option<String>,   // nombre, ruta "A/B" (case-insensitive) o id de carpeta
    pub stop_on_failure: bool,    // --bail
    pub delay_ms: u64,            // pausa entre requests
    pub iterations: u32,          // repetir el plan N veces
}

pub async fn run_collection(
    client: &HttpClient,
    collection: &Collection,
    environment: Option<&Environment>,
    options: &RunnerOptions,
    on_event: impl FnMut(&RunEvent),   // Started / RequestStarted / RequestFinished
) -> Result<RunSummary>;               // Err solo si la carpeta no existe
```

`RunSummary` incluye `total`, `passed`, `failed`, `errors`, `skipped`, `assertions_passed`, `assertions_failed`, `duration_ms`, `bailed`, `cancelled` y `results` (por request: status HTTP, tiempo, URL resuelta, assertions, error). Writers: `to_json()`, `to_junit_xml()` (un `<testcase>` por request; `<failure>` / `<error>` / `<skipped>`), `to_text()`. `plan_requests()` lista lo que se ejecutaría.

### Uso

```
:run                          # colección del sidebar / tab activo
:run "Demo API" Users --bail  # carpeta, parar en el primer fallo
:runexport report.xml         # JUnit XML (.json → JSON, .txt → texto)
R                             # en el sidebar, sobre una colección o carpeta
```

```bash
hitt test "Demo API" --env dev --folder Users --bail \
  --reporter junit --output report.xml   # exit code != 0 si falla alguna request
```

Opciones de `hitt test`: `--folder`, `--env`, `--bail`, `--reporter text|json|junit`, `--output <ruta>`, `--delay <ms>`, `--iterations <n>`. Con `--output` (o un reporter distinto de `text`) el progreso por request se imprime en stderr.

---

## Suite de tests del proyecto

### Estructura

Los tests de integración están en cada crate (los conteos cambian seguido; `cargo test --workspace` da el total):

```
hitt-core/tests/
├── assertions.rs          # Motor de aserciones
├── collection_runner.rs   # Collection runner contra un servidor local
├── core_models.rs         # Modelos de datos, helpers (unique_name, query params)
├── diff.rs                # Diff de texto
├── exporters.rs           # Exportadores (cURL, Markdown)
├── form_data.rs           # multipart form-data (valores `@archivo`)
├── history.rs             # HistoryStore y history.json
├── http_client.rs         # HttpClient contra un servidor local
├── importers.rs           # Importadores (Postman, HAR, OpenAPI, cURL, .env, chains)
├── postman_roundtrip.rs   # Round-trip Postman
├── storage.rs             # CollectionsStore, AppConfig
└── variables.rs           # Resolución de variables

hitt-tui/tests/
├── cli_commands.rs        # Subcomandos CLI (`hitt run`, `create`, `chain …`, `test`)
├── collection_runner.rs   # `:run`, modal del runner, `:runexport`
├── diff_selector.rs       # Modal de diff
├── environments.rs        # :newenv / :renameenv / :delenv / modal de variables
├── event_commands.rs      # Comandos `:` y teclas de normal mode
├── history_replay.rs      # Sidebar History, `:replay`
├── mouse.rs               # Clicks y scroll (hit-testing)
├── qa_fixes.rs            # Regresiones encontradas en QA
└── request_editors.rs     # Editores de params / headers / body / auth / assertions

*/tests/fixtures/
├── sample.har                      # HAR de ejemplo
├── petstore_openapi.yaml           # Spec OpenAPI para testing
├── sample.env                      # Archivo .env de ejemplo
├── sample_chain.yaml               # Chain definition de ejemplo
├── sample_postman_collection.json  # Colección Postman v2.1
└── sample_postman_environment.json # Environment Postman
```

### Patrones de testing

1. **Helper functions**: Cada archivo define helpers como `test_app()`, `make_response()`, `test_config()`
2. **Fixtures embebidos**: `include_str!("fixtures/sample.har")` para cargar datos de prueba
3. **Directorios temporales**: `tempfile::TempDir` para tests de persistencia aislados
4. **Builder pattern**: Construcción fluida de objetos complejos para tests
5. **Round-trip testing**: Import → export → import verifica consistencia
6. **Tests async**: Cobertura de operaciones async con `#[tokio::test]`

### Ejecutar tests

```bash
cargo test --workspace             # Todos los tests
cargo test -p hitt-core --test assertions   # Solo tests de aserciones
cargo test -- --nocapture  # Con output visible
```
