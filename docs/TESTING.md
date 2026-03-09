# Aserciones y pruebas de carga

> Ver también: [ARCHITECTURE.md](./ARCHITECTURE.md) para la visión general.

hitt incluye un motor de aserciones para validar responses y un framework de pruebas de carga integrado.

---

## Motor de aserciones

**Implementación:** `src/testing/assertion_engine.rs`

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

---

## JSON Schema validation

**Crate:** `jsonschema` 0.26
**Implementación:** `src/testing/schema_validator.rs`

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
**Implementación:** `src/testing/diff.rs`

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

**Implementación:** `src/testing/load_test.rs`

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

## Suite de tests del proyecto

### Estructura

```
tests/
├── assertions.rs          # 27 tests — Motor de aserciones
├── cli_commands.rs        # 14 tests — Comandos CLI
├── core_models.rs         # 32 tests — Modelos de datos
├── diff.rs                # 7 tests — Diff de texto
├── event_commands.rs      # 126 tests — Eventos e interacción UI
├── exporters.rs           # 13 tests — Exportadores
├── importers.rs           # 39 tests — Importadores
├── postman_roundtrip.rs   # 3 tests — Round-trip Postman
├── storage.rs             # 8 tests — Persistencia
└── variables.rs           # 16 tests — Resolución de variables

tests/fixtures/
├── sample.har                      # HAR de ejemplo
├── petstore_openapi.yaml           # Spec OpenAPI para testing
├── sample.env                      # Archivo .env de ejemplo
├── sample_chain.yaml               # Chain definition de ejemplo
├── sample_postman_collection.json  # Colección Postman v2.1
└── sample_postman_environment.json # Environment Postman
```

### Estadísticas

| Métrica | Valor |
|---------|-------|
| Total de tests | ~285 |
| Tests async (`#[tokio::test]`) | ~126 |
| Tests sync (`#[test]`) | ~159 |
| Archivo más grande | `event_commands.rs` (126 tests, 2046 líneas) |
| Líneas de código de tests | ~4,246 |
| Fixtures | 6 archivos |

### Patrones de testing

1. **Helper functions**: Cada archivo define helpers como `test_app()`, `make_response()`, `test_config()`
2. **Fixtures embebidos**: `include_str!("fixtures/sample.har")` para cargar datos de prueba
3. **Directorios temporales**: `tempfile::TempDir` para tests de persistencia aislados
4. **Builder pattern**: Construcción fluida de objetos complejos para tests
5. **Round-trip testing**: Import → export → import verifica consistencia
6. **Tests async**: Cobertura de operaciones async con `#[tokio::test]`

### Ejecutar tests

```bash
cargo test              # Todos los tests (~285)
cargo test assertions   # Solo tests de aserciones
cargo test -- --nocapture  # Con output visible
```
