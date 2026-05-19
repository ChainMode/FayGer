# Capítulo 8. Hoja de Ruta

Este capítulo define los límites de la primera fase y traza la evolución por direcciones conocidas. Cada elemento indica su alcance de impacto en la arquitectura actual para que los diseños futuros converjan.

## 8.1 Alcance de la primera fase

Una entrega de primera fase de Fayger satisface al menos:

- Contratos estables para las tres capas (Loader / Runtime / Adapter).
- Formato de artefacto BuF en cinco partes con Manifest / Section_Index en CBOR determinista.
- Cadena de carga: Read(Header/Manifest/Index) → Parse → Verify(Structural) → Verify(Digest of Header/Manifest/Index) → Verify(Signature) → Negotiate Version → Select(Sections by LoadProfile) → Resolve → Read(Selected Section Bodies)? → HandOff.
- Abstracción BuF_Source con soporte para cualquier backend (local, red, almacenamiento de objetos, definido por el usuario).
- Carga parcial (selección por perfil) y carga perezosa (Eager / Lazy LoadStrategy), cubriendo necesidades de tamaño y fetch en runtime de terminales restringidas (drones, cámaras).
- Camino de ejecución interpretado; máquina de estados de ciclo de vida de BuF_Instance y aislamiento de fallos.
- 8 categorías de Universal_Instruction y cuatro Platform_Adapters integrados (escritorio / servidor / navegador / In-App).
- Modelo de error unificado y bus de eventos de observabilidad; los errores del loader llevan etiqueta `phase`.
- Verificación de firma, modo de firma forzada y visibilidad de actualizaciones de raíces de confianza.

## 8.2 No objetivos de la primera fase

Para acotar la complejidad, no se entrega:

- Memoria compartida e IPC entre BuF_Instances.
- Compilación JIT y generación de código máquina.
- Planificación distribuida y colaboración multi-nodo de Fayger.
- Distribución diferencial de BuF y caché por capas entre artefactos (la carga parcial cubre la elección por Section dentro de un artefacto; la reutilización diferencial entre artefactos y un protocolo de Distribution se difieren).
- Repositorio remoto / protocolo Distribution integrado.

Un no objetivo no significa "no se necesita en el futuro", sino "no está hoy en la superficie del contrato". Las fases posteriores no deben romper contratos existentes.

## 8.3 Fase 2: rendimiento de ejecución

Dirección: añadir caminos de ejecución más eficientes sin romper Runtime_Interface.

### 8.3.1 Abstracción ExecutionStrategy

Extraer "cómo se ejecuta la semántica del BuF" como estrategia:

```idl
interface ExecutionStrategy {
  prepare(obj: BuFObject) -> PreparedExecutable
  step(exec: PreparedExecutable, ctx: ExecutionContext) -> StepResult
}
```

La fase 1 entrega `Interpreter` como una estrategia. Después pueden coexistir:

- `JIT`: compilar la representación intermedia de la code section a código máquina.
- `AOT`: producir código máquina al cargar (apto para release).
- `HybridTier`: interpretar → sondear → compilar.

El cambio de estrategia es interno a una Runtime_Implementation y no afecta el contrato externo de Runtime_Interface.

### 8.3.2 Observabilidad de rendimiento

Nuevas categorías de eventos:

- `ExecutionMetric`: muestreo de interpretación / compilación / GC.
- `TierTransition`: transición entre niveles de ejecución por capas.

Las estrategias concretas no se exponen externamente, pero los llamadores observan su efecto.

## 8.4 Fase 3: distribución y caché

### 8.4.1 Capas y chunks entre artefactos

La fase 1 ya soporta lectura bajo demanda con granularidad de Section dentro de un BuF (Lazy). Más adelante se introduce el compartido por capas entre artefactos:

- Recursos compartidos entre BuFs (librerías estándar, pesos de modelo) se extraen como capas independientes.
- El Manifest gana un `layer_refs` opcional que referencia Sections en otros artefactos.
- Un protocolo de distribución obtiene los recursos por capas de forma compartida; al haber acierto en caché local se reutiliza entre BuFs.

Requiere campos opcionales nuevos en el Manifest, lo que hace subir un nivel menor `schema_version`. Los BuFs existentes no se ven afectados (camino de compatibilidad en §8.7).

### 8.4.2 Protocolo de distribución

Introducir un protocolo ligero (referenciando OCI Distribution Spec):

- Tres pasos: `discover` / `pull` / `verify`.
- El protocolo no vive en el proceso de Fayger; un componente independiente lo implementa. Fayger consume solo los chunks que produce.

### 8.4.3 Caché local

Para BuFs ya cargados y verificados por firma, cachear el resultado de parseo de Manifest y Section entre procesos. En cada acierto se vuelve a verificar la firma (evitar envenenamiento).

## 8.5 Fase 4: colaboración entre instancias

### 8.5.1 Memoria compartida (controlada)

Permitir que varias BuF_Instances en el mismo Fayger compartan una región de memoria:

- Declarar explícitamente la capacidad `shared_memory` en el Manifest.
- La capa de ejecución asigna regiones centralmente y las expone según las concesiones.
- Las regiones compartidas están bajo monitoreo de cuotas; las violaciones disparan suspensión.

### 8.5.2 Protocolo IPC

Mensajería entre instancias (en el mismo proceso o el mismo Fayger):

- Expresada como nueva categoría `ipc` de Universal_Instruction.
- Denegado por defecto; el Manifest debe declarar el conjunto de IDs de los pares o los topics.

### 8.5.3 Ensamblaje de dependencias dentro de un solo Fayger

Permitir que un BuF referencie a otros BuFs como dependencias:

- La fase `Resolve` admite resolución recursiva.
- El grafo de dependencias se expresa con el campo `dependencies` del Manifest.
- Las dependencias cíclicas se rechazan en Resolve.

## 8.6 Fase 5: distribuido

Dirección: colaboración multi-nodo de Fayger. Fuera de la fase 1, pero hay que dejar margen para:

- Los IDs de BuF_Instance deben ser únicos y direccionables a través de nodos.
- Los timestamps de Lifecycle Events deben alinearse con sistemas de trazado externos.
- La codificación TLV de Universal_Instruction ya soporta transporte por red; el "despacho remoto" puede implementarse como Adapter.

## 8.7 Estrategia de compatibilidad

Toda evolución debe seguir estas reglas; si no, es un cambio rompedor:

1. **Versión de Runtime_Interface.** Subir según SemVer al añadir métodos o cambiar semántica. El BuF_Manifest expresa su mínimo con `runtime_interface_min`.
2. **Versión de schema BuF.** Añadir campos opcionales → bump menor; añadir campos requeridos o cambiar semántica → bump mayor.
3. **Estabilidad de códigos de error.** Los códigos publicados no se reutilizan ni cambian de semántica; pueden añadirse nuevos.
4. **Categorías de Universal_Instruction.** Las 8 actuales no reusan número; las nuevas usan bits reservados.
5. **Flujo de obsolescencia.** Marcar primero con `deprecation_notice`; al cargar se avisa pero se permite; quitar tras al menos una mayor.

## 8.8 Recomendaciones de tooling

Para sostener la evolución a largo plazo, recomendamos:

- **Comprobación estática de dirección de dependencias.** Prohibir que la capa de ejecución importe tipos Platform_Adapter o tipos internos del loader. Implementable con ArchUnit / dep-cruiser / lints propios.
- **Suite base de pruebas basadas en propiedades.** Fijar las propiedades de corrección del plano (equivalencia ida y vuelta, transiciones de máquina de estados, cadenas de error, álgebra de capacidades, …) como Conformance Suite multilenguaje para que cualquier Runtime_Implementation se autocertifique.
- **Validador de CBOR de Manifest.** Verificar que el Manifest usa CBOR determinista; las codificaciones no deterministas no son conformes.
- **Health-check de descriptores de Adapter.** Al registrar, verificar al instante la consistencia de `supported_capabilities` y `supported_instructions` (p. ej., el cierre mínimo de bits frente a las capacidades requeridas por instrucción).

## 8.9 Resumen de hoja de ruta

| Fase | Direcciones clave | Capa principal | Impacto en compatibilidad |
| --- | --- | --- | --- |
| Fase 1 (actual) | Contratos de tres capas, ejecución interpretada, cuatro adaptadores integrados, firmas, carga parcial por Section, Eager/Lazy, abstracción BuF_Source | Tres capas + transversal | — |
| Fase 2 | ExecutionStrategy / JIT / AOT | Runtime | Compatible (adiciones) |
| Fase 3 | Capas entre artefactos / Distribution / caché local | Loader | Compatible (menor) |
| Fase 4 | Memoria compartida / IPC / grafo de dependencias | Runtime + Loader | Compatible (adiciones) |
| Fase 5 | Colaboración multi-nodo | Adapter (despacho remoto) | Compatible (adiciones) |

Los documentos de diseño de fases posteriores se publicarán como planos independientes y compartirán terminología y contratos con este.
