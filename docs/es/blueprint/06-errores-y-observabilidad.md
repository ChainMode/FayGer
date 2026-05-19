# Capítulo 6. Errores y Observabilidad

El modelo de errores y la observabilidad son dos cables transversales a las tres capas de Fayger. Permiten que los llamadores externos comprendan los fallos, los ubiquen y monitoricen la ejecución con la misma semántica.

## 6.1 Modelo de error unificado

```idl
struct FaygerError {
  code: ErrorCode
  source_layer: Layer       // loader | runtime | adapter | host
  message: String
  context: Map<String, Value>
  cause: Option<Box<FaygerError>>
}
```

Semántica de campos:

- **code.** Identificador estable y documentable. Para decisiones del llamador; la semántica no cambia entre versiones.
- **source_layer.** Etiqueta la capa que originó el error. Facilita el enrutamiento al manual de diagnóstico correspondiente.
- **message.** Descripción legible. Breve, no duplica la información del code.
- **context.** Pares clave-valor estructurados orientados a herramientas. Como mínimo lleva los campos clave que dispararon el error (offset, missing_fields, current_state, expected_range, …).
- **cause.** Cadena causal entre capas. Al envolver un error, el inferior queda en `cause` para no perder información.

## 6.2 Reglas de producción y propagación

1. **Construir cerca del origen.** Cada capa construye `FaygerError` dentro de su frontera y etiqueta `source_layer`.
2. **Envolver capa a capa.** Si la capa superior añade semántica, debe preservar el error inferior en `cause` y no reescribir `source_layer`.
3. **No perder contexto.** `context` lleva los campos clave que dispararon el error.
4. **Preservar el fallo.** Una BuF_Instance que entra en `Failed` debe persistir la última Universal_Instruction y el error correspondiente en `failure_info` hasta que se destruya explícitamente.

## 6.3 Ejemplo de cadena de error

Una propagación típica de tres capas:

```
[top]  RT_INSTANCE_FAILED      (source_layer = runtime)
         └── cause:
[mid]    ADP_HOST_CALL_FAILED  (source_layer = adapter)
           └── cause:
[low]      <error nativo del host> (source_layer = host)
```

Recorrer `cause` produce la secuencia `[runtime, adapter, host]`, que coincide con el orden de aparición real "de dentro hacia fuera". Verificada con PBT.

## 6.4 Visión general de códigos de error clave

| Origen | Código | Disparador |
| --- | --- | --- |
| Loader | `LDR_PARSE_FAIL` | Stream BuF inválido |
| Loader | `LDR_DIGEST_MISMATCH` | Discrepancia de digest de Section (Eager) |
| Loader | `LDR_LAZY_DIGEST_MISMATCH` | Discrepancia en primer acceso Lazy |
| Loader | `LDR_SIGNATURE_FAIL` | Firma faltante en modo forzado / firma inválida |
| Loader | `LDR_SCHEMA_UNSUPPORTED` | Versión de schema fuera de rango soportado |
| Loader | `LDR_RUNTIME_VERSION_TOO_HIGH` | Mín. de Runtime_Interface mayor que el provisto |
| Loader | `LDR_MISSING_REQUIRED_FIELD` | Faltan campos requeridos al serializar |
| Loader | `LDR_PROFILE_REQUIRED_SECTION_MISSING` | Section requerida no cargable bajo el perfil actual |
| Loader | `LDR_LAZY_SOURCE_UNAVAILABLE` | BuF_Source no disponible en acceso Lazy posterior |
| Loader | `LDR_SOURCE_READ_FAILED` | Error de lectura de BuF_Source (Eager o Lazy) |
| Runtime | `RT_ILLEGAL_TRANSITION` | Transición ilegal de ciclo de vida |
| Runtime | `RT_IMPL_NOT_FOUND` | Sin implementación compatible |
| Runtime | `RT_IMPL_MISSING_METHODS` | A la implementación le faltan métodos requeridos |
| Runtime | `RT_QUOTA_EXCEEDED` | Uso de recursos excede cuota |
| Runtime | `RT_INSTANCE_FAILED` | La instancia entró en Failed |
| Adapter | `ADP_NO_MATCHING_PLATFORM` | Ningún adaptador coincide con el anfitrión |
| Adapter | `ADP_UNSUPPORTED_INSTRUCTION` | Instrucción no soportada |
| Adapter | `ADP_CAPABILITY_DENIED` | Capacidades insuficientes tras recorte |
| Adapter | `ADP_HOST_CALL_FAILED` | El anfitrión falló tras la traducción |

Los errores del loader etiquetan `context.phase` con `eager` o `lazy`, así el llamador puede enrutar por fase: los fallos en arranque pueden disparar fallback a otra fuente / perfil o reintento; los fallos Lazy en runtime suelen afectar solo al acceso a esa Section, y el llamador decide si degrada.

## 6.5 Bus de eventos de observabilidad

```idl
interface EventBus {
  publish(event: FaygerEvent)
  subscribe(filter: EventFilter, handler: EventHandler) -> SubscriptionId
  set_debug_enabled(enabled: bool)
}
```

Categorías de eventos:

- **Lifecycle Event.** Transición de estado de BuF_Instance, con `from`, `to`, `timestamp`, `instance_id`.
- **Quota Event.** Muestreo de recursos, exceso, suspensión.
- **Loader Event.** Entrada / completitud / fallo de fase, con nombre de fase.
- **Adapter Event.** Despacho de instrucción, resultado de recorte, registro de instrucción no soportada.
- **Debug Event.** Solo cuando `set_debug_enabled(true)`. Para diagnóstico profundo.

### Consistencia de la secuencia de eventos de ciclo de vida

Para cualquier secuencia legal `[(s₀, s₁), (s₁, s₂), …]`, la secuencia de eventos emitidos debe cumplir:

- Misma longitud.
- Cada `eᵢ.from == sᵢ₋₁`, `eᵢ.to == sᵢ`.
- `eᵢ.timestamp` es monótono no decreciente.

## 6.6 Integración con canales de log / trace del anfitrión

Cuando el anfitrión ofrece canales de log o trace (systemd-journal, OS Logger, console del navegador, canal de SDK In-App, …), la capa de adaptación mapea los eventos internos a ese canal:

- El mapeo lo autodescribe el Platform_Adapter; la capa de ejecución desconoce las diferencias.
- Niveles (debug / info / warn / error) corresponden a los del anfitrión.
- Lifecycle Event y Loader Event en info; errores Quota / Adapter en warn / error.

Si no hay canal de log, los eventos quedan en el EventBus para suscriptores.

## 6.7 Interruptor de eventos de depuración

La salida de eventos de depuración está apagada por defecto. Activación:

```idl
event_bus.set_debug_enabled(true)
```

Eventos de depuración incluyen:

- Detalle de despacho por instrucción.
- Entradas y salidas por recorte de capacidades.
- Lecturas por muestreo de recursos.

Por su volumen, los suscriptores de depuración deben filtrar, limitar o persistir activamente.

## 6.8 Interfaces de consulta de estado

Fayger debe exponer las siguientes consultas para operación y diagnóstico:

| Consulta | Devuelve |
| --- | --- |
| `state(instance_id)` | Lifecycle_State actual |
| `failure_info(instance_id)` | Última Universal_Instruction y objeto error que disparó el fallo |
| `list_implementations()` | Descriptores de Runtime_Implementations registradas |
| `current_adapter()` | Descriptor del Platform_Adapter actualmente seleccionado |
| `granted_capabilities(instance_id)` | Capacidades reales de la instancia tras recorte |

Estas interfaces son de solo lectura, idempotentes y llamables en cualquier estado.

## 6.9 Postura de seguridad por defecto

- El modo de firma forzada está **apagado** por defecto (comodidad de desarrollo); las builds de release deberían ponerlo **encendido** en la capa de configuración.
- Los eventos de depuración están **apagados** por defecto; activar explícitamente.
- Las capacidades no declaradas se **deniegan** por defecto para BuF.
- Las categorías de Universal_Instruction no reconocidas se **deniegan** por defecto; nada de "pasar en silencio".
