# Démarrage rapide avec Fayger

**Fayger** est l'environnement d'exécution virtuel multiplateforme (Virtual Runtime Environment) dédié à Fay (`iFay` et `coFay`). Il prend en charge les artefacts portables **BuF (Built-up Fay)** ainsi que les manifestes déclaratifs **FayManifest**, assurant la vérification cryptographique, la gestion du cycle de vie et l'adaptation contrôlée sur les hôtes de bureau, serveur, navigateur et embarqués.

> Précision terminologique : **Hôte (Host)** désigne exclusivement le terminal ou le processus applicatif ; la personne physique est toujours désignée par **Human Prime**.

## Architecture en trois couches

1. **Couche Loader** : Analyse les artefacts `BuF` et vérifie les empreintes, les signatures Ed25519 et la compatibilité des versions.
2. **Couche Runtime** : Implémente le contrat `Runtime_Interface` indépendant du langage et les quotas de ressources, vérifiés par la suite `fayger-conformance`.
3. **Couche Adapter** : Traduit les instructions `Universal_Instruction` sous contrôle d'autorisation CAP et de vérification à deux niveaux `Ego_Package`.

```bash
fayger version
fayger conformance
fayger demo live-ifay
```

Consultez le répertoire `blueprint/` pour l'architecture détaillée.
