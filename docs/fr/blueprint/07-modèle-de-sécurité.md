# Chapitre 7. Modèle de Sécurité

Le modèle de sécurité de Fayger repose sur deux fils :

- **Origine de confiance.** Un artefact BuF peut être signé ; au chargement, on vérifie l'origine et l'intégrité.
- **Exécution contrôlée.** Une BuF_Instance ne peut utiliser à l'exécution que les capacités explicitement accordées ; les capacités non déclarées sont totalement invisibles.

Les deux fils atterrissent respectivement dans la couche de chargement et la couche d'adaptation, renforcés par le modèle d'erreurs du chapitre 6 et l'isolation de défaillance du chapitre 4.

## 7.1 Origine de confiance : signatures et digests

### Digests

Chaque Section déclare un `digest` dans le BuF_Manifest. La phase `Verify Digest` recalcule et compare par Section :

- Toute non-concordance retourne `LDR_DIGEST_MISMATCH`.
- Le contexte porte la `section_id` fautive.

Le `manifest_digest` du Trailer vérifie en même temps que le Manifest n'a pas été tronqué ou modifié.

### Périmètre de signature

Octets couverts par la signature :

```
Header || Manifest_minus_signature || Section_Index
```

Intention :

- Même si un attaquant modifie un champ du Manifest après signature (sauf le bloc signature), la signature est invalidée.
- Modifier l'offset / la longueur / le digest des Sections invalide aussi la signature, car Section_Index est dans le périmètre.
- Les corps de Section ne sont pas directement couverts, mais leurs `digest` le sont via le Manifest, donc indirectement.

Cette approche « signer le manifest, chaîner les digests vers les sections » est le même compromis qu'OCI Image : taille d'entrée maîtrisée et temps de vérification prévisible.

### Algorithmes de signature supportés (phase 1)

- `ed25519`
- `ecdsa-p256`
- `rsa-pss-sha256`

L'identifiant d'algorithme et la référence de clé publique sont déclarés explicitement dans le bloc `signature` du Manifest, évitant les négociations implicites basées sur ASN.1 / X.509.

### Tableau de décision de signature

| Condition | Résultat |
| --- | --- |
| enforce_signature on ∧ pas de signature | `Err(LDR_SIGNATURE_FAIL)`, raison `MissingSignature` |
| signature présente ∧ vérification échoue | `Err(LDR_SIGNATURE_FAIL)`, raison `InvalidSignature` |
| signature présente ∧ vérification ok | `Ok` |
| enforce_signature off ∧ pas de signature | `Ok` (commodité de développement) |

## 7.2 Mode signature obligatoire

Le mode signature obligatoire (enforce signature) est un commutateur de la politique de chargement, contrôlé par `LoaderPolicy.require_signature` :

```idl
struct LoaderPolicy {
  require_signature: bool
  trusted_roots: TrustedRootSet
  allowed_schema_versions: VersionRange
}
```

Comportement :

- Activé : tout BuF doit porter une signature et la vérification doit passer ; sinon le chargement est refusé.
- Désactivé : un BuF sans signature se charge (commodité) ; un BuF à signature invalide est rejeté (« faire semblant de signer » est interdit).

Pour les contextes de release (production, canaux de distribution de confiance), nous recommandons `require_signature` à `true` par défaut au niveau configuration.

## 7.3 Gestion des racines de confiance

```idl
interface TrustStore {
  current_roots() -> TrustedRootSet
  update(roots: TrustedRootSet)
  enforce_signature() -> bool
}
```

Contrainte de visibilité des mises à jour :

- Pour une séquence `Ops` mêlant `Update(roots)` et `Load(buf)`, chaque `Load(buf)` utilise exactement les racines fixées par la dernière `Update` antérieure.
- Cela rend sémantiquement impossible « changer les racines en plein chargement ».

Stratégie d'implémentation :

- À chaque entrée dans `LoaderPipeline.load`, prendre un snapshot du `TrustStore` ; tout le chargement utilise le snapshot.
- Interdire explicitement le partage d'un ensemble de racines mutables pendant le chargement.

## 7.4 Exécution contrôlée : modèle de sécurité par capacités

La couche d'adaptation utilise la sécurité par capacités (comme WASI). BuF déclare ses requêtes de capacités dans le Manifest ; Platform_Adapter et la politique d'hôte décident conjointement ce qui est accordé.

Formel :

```
granted = requested ∩ available ∩ host_policy
denied  = requested \ granted
```

### Porte de démarrage

Si `manifest.required_capabilities \ granted ≠ ∅` :

- `start()` doit retourner une erreur.
- `context.missing` égal à la différence.
- L'instance n'entre pas en Running.

C'est la règle dure « pas de démarrage si capacités insuffisantes ».

### Refus par défaut

- Les capacités non déclarées sont invisibles pour BuF.
- Un Platform_Adapter ne doit pas fournir de capacités non déclarées sous prétexte que « les bits semblent corrects ».
- Les catégories Universal_Instruction inconnues retournent `ADP_UNSUPPORTED_INSTRUCTION` ; aucun « passage silencieux ».

## 7.5 Classification des capacités et notes de réduction (par hôte)

| Classe d'hôte | Habituellement disponibles | Habituellement indisponibles | Note |
| --- | --- | --- | --- |
| Bureau | `io.*`, `net.*`, `ui.*`, `time`, `random`, `crypto`, `proc.*`, `host.*` | dépend de plate-forme/permissions | quasi complet |
| Serveur | `io.*`, `net.*`, `time`, `random`, `crypto`, `proc.*` | `ui.*` désactivé | sans GUI par défaut |
| Navigateur | `net.fetch`, `net.websocket`, `ui.dom`, `time`, `random`, sous-ensemble de `crypto` | `proc.*`, majorité de `io.*` | réduction forte |
| In-App | injectées explicitement par l'hôte | tout refusé par défaut | le plus strict ; l'hôte a le dernier mot |

## 7.6 Isolation des ressources comme barrière de sécurité

L'isolation des ressources est aussi un élément de la barrière de sécurité :

- Un échec d'une BuF_Instance ne se propage pas (propriété d'isolation du chapitre 4).
- Le dépassement de quota déclenche la suspension, empêchant une instance d'épuiser les ressources hôte.
- Les RuntimeDataAreas des instances ne sont pas mutuellement visibles, prévenant les fuites par canal latéral au niveau données.

## 7.7 Interaction sécurité / erreurs

Les défaillances liées à la signature et aux capacités sont renvoyées via le modèle unifié :

- Échec de signature : `LDR_SIGNATURE_FAIL`, avec `MissingSignature` / `InvalidSignature`.
- Capacité refusée : `ADP_CAPABILITY_DENIED`, avec l'ensemble refusé.
- Échec de la porte de démarrage : réutilise `ADP_CAPABILITY_DENIED` ; `context.missing` porte la différence.

Les chaînes suivent le chapitre 6 : l'erreur inférieure va dans `cause` ; l'erreur supérieure marque son `source_layer` ; les appelants peuvent parcourir la chaîne pour localiser.

## 7.8 Recommandations d'audit

L'audit n'est pas dans le périmètre interne de Fayger, mais le bus d'événements, les Lifecycle Events et les Loader Events offrent suffisamment d'ancres aux systèmes d'audit externes :

- Événements succès / échec de chaque phase de chargement comme points d'audit d'origine.
- Lifecycle Events documentant les chronologies start / suspend / terminate par instance.
- Résultats de réduction de capacités comme points d'audit des décisions de sécurité.
- Échec de signature, refus de capacités et dépassement de quota peuvent être abonnés par SIEM / pipelines externes.
