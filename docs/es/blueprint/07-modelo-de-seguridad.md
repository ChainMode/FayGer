# Capítulo 7. Modelo de Seguridad

El modelo de seguridad de Fayger se apoya en dos hilos:

- **Origen confiable.** Un artefacto BuF puede estar firmado; en la carga se verifica origen e integridad.
- **Ejecución controlada.** Una BuF_Instance solo puede usar las capacidades concedidas explícitamente; las no declaradas son del todo invisibles.

Ambos hilos aterrizan en la capa de carga y la capa de adaptación respectivamente, reforzados por el modelo de errores del Capítulo 6 y el aislamiento de fallos del Capítulo 4.

## 7.1 Origen confiable: firmas y digests

### Digests

Cada Section declara un `digest` en el BuF_Manifest. La fase `Verify Digest` recalcula y compara por Section:

- Cualquier discrepancia devuelve `LDR_DIGEST_MISMATCH`.
- El context incluye la `section_id` fallida.

`manifest_digest` del Trailer verifica al mismo tiempo que el propio Manifest no fue truncado o modificado.

### Alcance de la firma

Bytes cubiertos por la firma:

```
Header || Manifest_minus_signature || Section_Index
```

Intención de diseño:

- Aunque un atacante modifique cualquier campo del Manifest tras la firma (excepto el bloque de firma), la firma se invalida.
- Modificar offset / length / digest de Sections también invalida la firma, porque Section_Index está en el alcance.
- Los cuerpos de Section no están cubiertos directamente, pero sus `digest` lo están en el Manifest, así que indirectamente también.

Este enfoque "firmar el manifest, encadenar digests a sections" es el mismo trade-off que la firma de OCI Image: tamaño de entrada controlado y tiempo de verificación predecible.

### Algoritmos de firma soportados (fase 1)

- `ed25519`
- `ecdsa-p256`
- `rsa-pss-sha256`

El identificador de algoritmo y la referencia a la clave pública se declaran explícitamente en el bloque `signature` del Manifest, evitando negociación implícita basada en ASN.1 / X.509.

### Tabla de decisión de la firma

| Condición | Resultado |
| --- | --- |
| enforce_signature on ∧ sin firma | `Err(LDR_SIGNATURE_FAIL)`, razón `MissingSignature` |
| firma presente ∧ verificación falla | `Err(LDR_SIGNATURE_FAIL)`, razón `InvalidSignature` |
| firma presente ∧ verificación ok | `Ok` |
| enforce_signature off ∧ sin firma | `Ok` (comodidad de desarrollo) |

## 7.2 Modo de firma forzada

El modo de firma forzada (enforce signature) es un interruptor de la política de carga, controlado por `LoaderPolicy.require_signature`:

```idl
struct LoaderPolicy {
  require_signature: bool
  trusted_roots: TrustedRootSet
  allowed_schema_versions: VersionRange
}
```

Comportamiento:

- Encendido: cualquier BuF debe llevar firma y verificarse; si no, se rechaza la carga.
- Apagado: un BuF sin firma se carga (comodidad); un BuF con firma inválida se rechaza ("aparentar firmar" no se permite).

Para entornos de release (producción, canales confiables) recomendamos por configuración `require_signature` por defecto en `true`.

## 7.3 Gestión de raíces de confianza

```idl
interface TrustStore {
  current_roots() -> TrustedRootSet
  update(roots: TrustedRootSet)
  enforce_signature() -> bool
}
```

Restricción de visibilidad de actualizaciones:

- Para una secuencia `Ops` con `Update(roots)` y `Load(buf)` mezclados, cada `Load(buf)` usa exactamente las raíces fijadas por la última `Update` previa.
- La restricción hace semánticamente imposible "cambiar raíces a mitad de carga".

Estrategia de implementación:

- En cada entrada a `LoaderPipeline.load`, tomar un snapshot de `TrustStore`; toda la carga usa el snapshot.
- Prohibir explícitamente compartir un conjunto de raíces mutable durante la carga.

## 7.4 Ejecución controlada: modelo de seguridad por capacidades

La capa de adaptación usa seguridad basada en capacidades (igual que WASI). BuF declara solicitudes de capacidades en el Manifest; Platform_Adapter y la política del anfitrión deciden conjuntamente lo concedido.

Formal:

```
granted = requested ∩ available ∩ host_policy
denied  = requested \ granted
```

### Compuerta de inicio

Si `manifest.required_capabilities \ granted ≠ ∅`:

- `start()` debe devolver error.
- `context.missing` igual a la diferencia.
- La instancia no entra en Running.

Es la regla dura "no iniciar si faltan capacidades".

### Denegar por defecto

- Las capacidades no declaradas son invisibles para BuF.
- Un Platform_Adapter no debe proveer capacidades no declaradas porque "los bits parezcan correctos".
- Categorías de Universal_Instruction no reconocidas devuelven `ADP_UNSUPPORTED_INSTRUCTION`; nada de "pasar en silencio".

## 7.5 Clasificación de capacidades y notas de recorte (por anfitrión)

| Clase | Disponibles típicas | No disponibles típicas | Nota |
| --- | --- | --- | --- |
| Escritorio | `io.*`, `net.*`, `ui.*`, `time`, `random`, `crypto`, `proc.*`, `host.*` | depende de plataforma / permisos | casi completas |
| Servidor | `io.*`, `net.*`, `time`, `random`, `crypto`, `proc.*` | `ui.*` deshabilitado | sin GUI por defecto |
| Navegador | `net.fetch`, `net.websocket`, `ui.dom`, `time`, `random`, subconjunto de `crypto` | `proc.*`, mayoría de `io.*` | recorte fuerte |
| In-App | inyectadas explícitamente por el anfitrión | denegadas por defecto | el más estricto; el anfitrión tiene la última palabra |

## 7.6 Aislamiento de recursos como barrera de seguridad

El aislamiento de recursos es a la vez un requisito de estabilidad y parte de la barrera de seguridad:

- Un fallo de una BuF_Instance no se propaga a otras (propiedad de aislamiento del Capítulo 4).
- Exceder cuota dispara suspensión, evitando que una instancia agote los recursos del anfitrión.
- Las RuntimeDataAreas de instancias no son mutuamente visibles, evitando filtraciones por canal lateral en la capa de datos.

## 7.7 Interacción seguridad / errores

Los fallos relacionados con firma y capacidades vuelven por el modelo unificado:

- Fallo de firma: `LDR_SIGNATURE_FAIL`, con `MissingSignature` / `InvalidSignature`.
- Capacidad denegada: `ADP_CAPABILITY_DENIED`, con el conjunto denegado.
- Fallo de la compuerta de inicio: reutiliza `ADP_CAPABILITY_DENIED`; `context.missing` lleva la diferencia.

Las cadenas de error siguen el Capítulo 6: el inferior va en `cause`; el superior etiqueta su `source_layer`; los llamadores recorren la cadena para localizar.

## 7.8 Recomendaciones de auditoría

La auditoría no es del alcance interno de Fayger, pero el bus de eventos, los Lifecycle Events y los Loader Events ya dan a sistemas de auditoría externos anclajes suficientes:

- Eventos éxito / fallo de cada fase de carga como puntos de auditoría de origen.
- Lifecycle Events registran las líneas de tiempo de inicio / suspensión / terminación de cada instancia.
- Los resultados de recorte como puntos de auditoría de decisiones de seguridad.
- Eventos de fallo de firma, denegación de capacidades y exceso de cuota pueden ser suscritos por SIEM / pipelines externos.
