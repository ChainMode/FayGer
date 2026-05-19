# Chapitre 6. Erreurs et Observabilité

Le modèle d'erreurs et l'observabilité sont deux fils transverses aux trois couches de Fayger. Ils permettent aux appelants externes de comprendre les défaillances, de les localiser et de superviser l'exécution avec la même sémantique.

## 6.1 Modèle d'erreurs unifié

```idl
struct FaygerError {
  code: ErrorCode
  source_layer: Layer       // loader | runtime | adapter | host
  message: String
  context: Map<String, Value>
  cause: Option<Box<FaygerError>>
}
```

Sémantique des champs :

- **code.** Identifiant stable et documentable. Pour le branchement côté appelant ; la sémantique ne change pas entre versions.
- **source_layer.** Marque la couche qui a originellement produit l'erreur. Aide au routage vers le manuel de diagnostic adéquat.
- **message.** Description lisible. Brève ; ne duplique pas le code.
- **context.** Paires clé-valeur structurées, orientées outil. Au minimum les champs déclencheurs (offset, missing_fields, current_state, expected_range, …).
- **cause.** Chaîne causale entre couches. En enveloppant, on place l'erreur inférieure dans `cause` pour ne pas perdre d'information.

## 6.2 Règles de production et de propagation

1. **Construire au plus près.** Chaque couche construit `FaygerError` dans son périmètre et marque `source_layer`.
2. **Envelopper couche par couche.** Si la couche supérieure ajoute du sens, elle préserve l'erreur inférieure dans `cause` et ne réécrit pas `source_layer`.
3. **Ne pas perdre le contexte.** `context` porte les champs déclencheurs.
4. **Préserver la défaillance.** Une BuF_Instance entrant en `Failed` doit conserver dans `failure_info` la dernière Universal_Instruction et l'erreur correspondante jusqu'à destruction explicite.

## 6.3 Exemple de chaîne d'erreurs

Une propagation tri-couche typique :

```
[top]  RT_INSTANCE_FAILED      (source_layer = runtime)
         └── cause:
[mid]    ADP_HOST_CALL_FAILED  (source_layer = adapter)
           └── cause:
[low]      <erreur native hôte> (source_layer = host)
```

Parcourir `cause` donne `[runtime, adapter, host]`, ce qui correspond à l'ordre réel « de l'intérieur vers l'extérieur ». Vérifié par PBT.

## 6.4 Aperçu des principaux codes d'erreur

| Origine | Code | Déclencheur |
| --- | --- | --- |
| Loader | `LDR_PARSE_FAIL` | Flux BuF invalide |
| Loader | `LDR_DIGEST_MISMATCH` | Mismatch de digest de Section (Eager) |
| Loader | `LDR_LAZY_DIGEST_MISMATCH` | Mismatch au premier accès Lazy |
| Loader | `LDR_SIGNATURE_FAIL` | Signature manquante en mode obligatoire / signature invalide |
| Loader | `LDR_SCHEMA_UNSUPPORTED` | Version de schema hors plage supportée |
| Loader | `LDR_RUNTIME_VERSION_TOO_HIGH` | Min Runtime_Interface supérieur au fourni |
| Loader | `LDR_MISSING_REQUIRED_FIELD` | Champs requis manquants à la sérialisation |
| Loader | `LDR_PROFILE_REQUIRED_SECTION_MISSING` | Section requise non chargeable sous le profil courant |
| Loader | `LDR_LAZY_SOURCE_UNAVAILABLE` | BuF_Source indisponible lors d'un accès Lazy ultérieur |
| Loader | `LDR_SOURCE_READ_FAILED` | Erreur de lecture BuF_Source (Eager ou Lazy) |
| Runtime | `RT_ILLEGAL_TRANSITION` | Transition illégale du cycle de vie |
| Runtime | `RT_IMPL_NOT_FOUND` | Routage sans implémentation correspondante |
| Runtime | `RT_IMPL_MISSING_METHODS` | Implémentation enregistrée à laquelle il manque des méthodes |
| Runtime | `RT_QUOTA_EXCEEDED` | Utilisation excédant le quota |
| Runtime | `RT_INSTANCE_FAILED` | L'instance est entrée en Failed |
| Adapter | `ADP_NO_MATCHING_PLATFORM` | Aucun adaptateur ne correspond à l'hôte |
| Adapter | `ADP_UNSUPPORTED_INSTRUCTION` | L'adaptateur courant ne supporte pas l'instruction |
| Adapter | `ADP_CAPABILITY_DENIED` | Capacités insuffisantes après réduction |
| Adapter | `ADP_HOST_CALL_FAILED` | Échec côté hôte après traduction en appel système |

Les erreurs du loader marquent `context.phase` à `eager` ou `lazy`, permettant à l'appelant de router par phase : un échec au démarrage peut déclencher un fallback vers une autre source / un autre profil ou un retry ; un échec Lazy à l'exécution n'affecte généralement que l'accès à cette Section, et l'appelant décide de la dégradation.

## 6.5 Bus d'événements d'observabilité

```idl
interface EventBus {
  publish(event: FaygerEvent)
  subscribe(filter: EventFilter, handler: EventHandler) -> SubscriptionId
  set_debug_enabled(enabled: bool)
}
```

Catégories :

- **Lifecycle Event.** Transition d'état d'une BuF_Instance, avec `from`, `to`, `timestamp`, `instance_id`.
- **Quota Event.** Échantillonnage de ressources, dépassement, suspension.
- **Loader Event.** Entrée / fin / échec de phase, avec nom de phase.
- **Adapter Event.** Dispatch d'instruction, résultat de réduction, enregistrement d'instruction non supportée.
- **Debug Event.** Émis seulement si `set_debug_enabled(true)`. Pour diagnostic poussé.

### Cohérence de la séquence d'événements de cycle de vie

Pour toute séquence légale `[(s₀, s₁), (s₁, s₂), …]`, la séquence d'événements émise doit :

- Avoir la même longueur.
- Pour chaque `eᵢ`, `eᵢ.from == sᵢ₋₁`, `eᵢ.to == sᵢ`.
- `eᵢ.timestamp` est monotone non décroissant.

## 6.6 Intégration avec les canaux log / trace de l'hôte

Quand l'hôte fournit des canaux log ou trace (systemd-journal, OS Logger, console du navigateur, canal SDK In-App, …), la couche d'adaptation y mappe les événements internes :

- Le mapping est auto-décrit par le Platform_Adapter ; la couche d'exécution ignore les différences.
- Niveaux (debug / info / warn / error) correspondent aux niveaux hôte.
- Lifecycle Event et Loader Event en info ; erreurs Quota / Adapter en warn / error.

Sans canal hôte, les événements restent dans l'EventBus pour les abonnés.

## 6.7 Interrupteur des événements de débogage

La sortie d'événements de débogage est désactivée par défaut. Activation :

```idl
event_bus.set_debug_enabled(true)
```

Inclut :

- Détails de dispatch par instruction.
- Entrées/sorties de chaque réduction de capacités.
- Lectures de chaque échantillonnage de ressources.

Vu le volume, les abonnés débogage doivent filtrer, limiter le débit ou persister activement.

## 6.8 Interfaces de requête d'état

Fayger doit exposer pour l'exploitation et le diagnostic :

| Requête | Retourne |
| --- | --- |
| `state(instance_id)` | Lifecycle_State courant |
| `failure_info(instance_id)` | Dernière Universal_Instruction et objet d'erreur ayant déclenché Failed |
| `list_implementations()` | Descripteurs des Runtime_Implementations enregistrées |
| `current_adapter()` | Descripteur du Platform_Adapter actuellement choisi |
| `granted_capabilities(instance_id)` | Capacités effectives de l'instance après réduction |

Ces interfaces sont en lecture seule, idempotentes et appelables dans n'importe quel état.

## 6.9 Posture de sécurité par défaut

- Le mode signature obligatoire est **désactivé** par défaut (commodité de développement) ; les builds release devraient l'**activer** par défaut au niveau configuration.
- Les événements de débogage sont **désactivés** par défaut ; activer explicitement.
- Les capacités non déclarées sont **refusées** par défaut pour BuF.
- Les catégories Universal_Instruction inconnues sont **refusées** par défaut ; aucun « passage silencieux » autorisé.
