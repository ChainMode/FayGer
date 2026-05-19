# Capítulo 1. Introducción

## 1.1 Antecedentes

El ecosistema Fay necesita un sustrato de ejecución que oculte las diferencias entre terminales y anfitriones, y que permita ejecutar el mismo artefacto Fay de forma consistente en escritorios, servidores, navegadores y anfitriones embebidos. Ese sustrato es **Fayger**.

Por analogía:

- Lo que la JVM es para el bytecode Java, **Fayger** lo es para BuF.
- Lo que Docker / OCI Runtime es para imágenes de contenedor, **Fayger** lo es para BuF.

Fayger no prescribe cómo se escribe la semántica de Fay. Solo prescribe cómo se carga, verifica, planifica y ejecuta un artefacto Fay, y cómo se aísla del anfitrión.

## 1.2 Posicionamiento

Fayger es un **entorno de ejecución virtual (Virtual Runtime Environment)**:

- Acepta como entrada una entidad empaquetada llamada **BuF (Built-up Fay)**.
- Ejecuta lectura, verificación estructural, negociación de versión, verificación de firma y carga.
- Ejecuta el BuF interpretadamente en una de varias implementaciones lingüísticas de la capa de ejecución.
- A través de la capa de adaptación, traduce las "instrucciones universales" producidas durante la ejecución a las capacidades del sistema de la terminal de destino.

Desde la perspectiva del usuario: entrega el BuF a Fayger y Fayger lo ejecuta.

## 1.3 Principios de diseño

La primera fase de Fayger se diseña con estas prioridades:

1. **Portabilidad primero.** Un BuF se compila una vez y corre en cualquier anfitrión adaptado. Las capas de ejecución y adaptación están desacopladas mediante una *Universal_Instruction* estable.
2. **Implementaciones multilenguaje.** La capa de ejecución expone una *Runtime_Interface* neutra al lenguaje, permitiendo que coexistan e intercambien implementaciones en Rust, Go, TypeScript y otros, sin atarse a un único ecosistema.
3. **Corrección demostrable.** El parseo y la serialización de BuF cumplen equivalencia de ida y vuelta; la máquina de estados del ciclo de vida cumple propiedades de transición legal; el comportamiento entre plataformas cumple equivalencia de secuencias. Estas propiedades se enuncian de modo que sean verificables con pruebas basadas en propiedades (PBT).
4. **Seguro y observable por defecto.** Valores por defecto seguros (modo de firma forzada disponible), por defecto silencioso (eventos de depuración apagados) y errores estructurados (modelo unificado con etiqueta de capa de origen y cadena de causas).

## 1.4 Visión general de tres capas

Fayger se divide verticalmente en tres capas:

- **Loader Layer.** Frontera entre BuF y el mundo exterior. Responsable de leer, escribir, verificar y negociar versiones.
- **Runtime Layer.** Ejecutor de la semántica de BuF. Soporta varias implementaciones lingüísticas intercambiables a través de un contrato unificado.
- **Adapter Layer.** Frontera entre Fayger y el anfitrión. Responsable de la traducción bidireccional entre instrucciones universales y llamadas al sistema.

Cada capa tiene un contrato externo claro y una división interna del trabajo. Los detalles vienen en el siguiente capítulo.

## 1.5 Mapeo a sistemas existentes

| Sistema existente | Equivalente en Fayger |
| --- | --- |
| JVM ClassLoader → Verifier → ExecutionEngine | Loader Layer → Runtime Layer |
| Class file de JVM (magic / version / constant pool) | Artefacto BuF (Header / Manifest / Sections / Trailer) |
| OCI Image Manifest + Layers | BuF_Manifest + Sections |
| OCI Runtime Spec | Runtime_Interface |
| containerd vs runc | Orquestación Runtime_Layer vs ejecución Runtime_Implementation |
| WASI capability-based security | Recorte de capacidades en Adapter_Layer |
| JNI / extism host functions | Categoría `host` de Universal_Instruction |

## 1.6 Glosario

- **Fayger.** El entorno de ejecución virtual definido por este plano.
- **BuF (Built-up Fay).** La entidad distribuible y cargable de Fay. Unidad de entrada de Fayger.
- **BuF_Manifest.** Catálogo de metadatos dentro de un BuF (versiones, entrada, dependencias, capacidades, cuotas, firma, …).
- **Loader_Layer / Runtime_Layer / Adapter_Layer.** Las tres capas de Fayger.
- **Runtime_Interface.** Contrato externo estable de la capa de ejecución.
- **Runtime_Implementation.** Implementación lingüística concreta de la capa de ejecución.
- **Platform_Adapter.** Adaptador que apunta a una terminal / SO en la capa de adaptación.
- **Universal_Instruction.** Instrucción independiente de plataforma entre las capas de ejecución y adaptación.
- **BuF_Instance.** Una entidad BuF cargada en la capa de ejecución que es ejecutable o se está ejecutando.
- **Lifecycle_State.** Estado del ciclo de vida de una BuF_Instance.
- **Host_Environment.** La terminal, sistema operativo o proceso de aplicación anfitriona donde Fayger realmente corre.
- **BuF_Source.** Abstracción del backend de almacenamiento para BuF (archivo local, red, almacenamiento de objetos, backends definidos por el usuario). Expone como mínimo lectura de acceso aleatorio.
- **LoadProfile.** Perfil del entorno de ejecución que el llamador entrega al cargar. Dirige la selección de Sections. Incluye al menos clase de terminal, conjunto de capacidades, límites de tamaño y bits de características de runtime.
- **SectionVisibility.** Nivel de visibilidad de una Section: `Required` (faltar implica fallo) o `Optional` (faltar implica degradación).
- **LoadStrategy.** Estrategia de fetch del cargador: `Eager` (lee todas las Sections seleccionadas por adelantado) o `Lazy` (lee solo cabecera + índice; los cuerpos de Section se obtienen al primer acceso).

## 1.7 Organización del plano

Los capítulos siguientes vienen en este orden:

- Capítulo 2. Arquitectura general
- Capítulo 3. Capa de carga
- Capítulo 4. Capa de ejecución
- Capítulo 5. Capa de adaptación
- Capítulo 6. Errores y observabilidad
- Capítulo 7. Modelo de seguridad
- Capítulo 8. Hoja de ruta
