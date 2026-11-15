# Primeros pasos con Fayger

**Fayger** es el entorno virtual de ejecución multiplataforma (Virtual Runtime Environment) para Fay (`iFay` y `coFay`). Procesa artefactos portátiles **BuF (Built-up Fay)** y manifiestos declarativos **FayManifest**, ofreciendo verificación criptográfica, gestión determinista del ciclo de vida y adaptación segura en hosts de escritorio, servidor, navegador y sistemas embebidos.

> Nota terminológica: **Host** se refiere únicamente al dispositivo terminal o proceso de aplicación; la persona natural siempre se denomina **Human Prime**.

## Arquitectura de tres capas

1. **Capa Loader**: Analiza artefactos `BuF` y verifica resúmenes SHA-256, firmas Ed25519 y compatibilidad de versiones.
2. **Capa Runtime**: Ejecuta la interfaz neutral `Runtime_Interface` y el aislamiento de cuotas, validado mediante la suite de conformidad `fayger-conformance`.
3. **Capa Adapter**: Traduce flujos `Universal_Instruction` bajo compuertas de autorización CAP y verificación de paquetes firmados `Ego_Package`.

```bash
fayger version
fayger conformance
fayger demo live-ifay
```

Consulte el directorio `blueprint/` para el diseño arquitectónico completo.
