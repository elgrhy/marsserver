# Apollo Helm Chart

Production-quality Helm chart for the **APOLLO AI Agent Execution Platform v2.2**.

## Components

| Component | Binary | Default Port | Kind | State |
|-----------|--------|-------------|------|-------|
| apollo-node | `apollo` | 8080 (HTTP) / 8443 (HTTPS) | StatefulSet | Stateful (PVC) |
| apollo-hub | `apollo-hub` | 9191 | Deployment | Stateless |
| apollo-operator | `apollo-operator` | — | Deployment | Stateless |

## Prerequisites

- Kubernetes >= 1.26
- Helm >= 3.10
- A default StorageClass with `ReadWriteOnce` support (for node persistence)
- `kubectl` configured against your target cluster

## Quick Start

### 1. Add the chart (local install from repo)

```bash
# From the repository root
cd /path/to/apollo
helm install apollo ./deploy/helm/apollo \
  --namespace apollo \
  --create-namespace \
  --set node.secretKeys="your-api-key-here" \
  --set node.jwtSecret="your-jwt-secret-here" \
  --set hub.hubKey="your-hub-key-here"
```

### 2. Verify the installation

```bash
kubectl get pods -n apollo
kubectl get svc  -n apollo

# Check node health
kubectl port-forward -n apollo svc/apollo-node 8080:8080 &
curl -H "X-Apollo-Key: your-api-key-here" http://localhost:8080/health
```

### 3. Test the hub

```bash
kubectl port-forward -n apollo svc/apollo-hub 9191:9191 &
curl http://localhost:9191/summary
```

## Production Installation

### Using an external Secret (recommended)

Do not embed secrets in values files. Create a K8s Secret first:

```bash
kubectl create secret generic apollo-credentials \
  --namespace apollo \
  --from-literal=secret-keys="key1,key2" \
  --from-literal=jwt-secret="$(openssl rand -hex 32)" \
  --from-literal=hub-key="$(openssl rand -hex 32)"
```

Then install referencing the Secret:

```bash
helm install apollo ./deploy/helm/apollo \
  --namespace apollo \
  --create-namespace \
  --set node.secretKeysSecretRef.name=apollo-credentials \
  --set node.secretKeysSecretRef.key=secret-keys \
  --set node.jwtSecretRef.name=apollo-credentials \
  --set node.jwtSecretRef.key=jwt-secret \
  --set hub.hubKeySecretRef.name=apollo-credentials \
  --set hub.hubKeySecretRef.key=hub-key
```

### With TLS (cert-manager)

```bash
helm install apollo ./deploy/helm/apollo \
  --namespace apollo \
  --create-namespace \
  -f values-prod.yaml \
  --set node.tls.enabled=true \
  --set node.tls.secretName=apollo-node-tls \
  --set ingress.enabled=true \
  --set ingress.hosts[0].host=apollo.example.com \
  --set ingress.tls[0].secretName=apollo-tls \
  --set ingress.tls[0].hosts[0]=apollo.example.com
```

### With autoscaling

```bash
helm install apollo ./deploy/helm/apollo \
  --namespace apollo \
  --create-namespace \
  -f values-prod.yaml \
  --set autoscaling.enabled=true \
  --set autoscaling.minReplicas=2 \
  --set autoscaling.maxReplicas=20 \
  --set autoscaling.targetCPUUtilizationPercentage=65
```

## Upgrading

```bash
helm upgrade apollo ./deploy/helm/apollo \
  --namespace apollo \
  --reuse-values \
  --set node.image.tag="2.3.0"
```

## Uninstalling

```bash
helm uninstall apollo --namespace apollo

# The PVC and Secret are retained by default (helm.sh/resource-policy: keep).
# To delete them manually:
kubectl delete pvc -n apollo -l app.kubernetes.io/instance=apollo
kubectl delete secret -n apollo apollo-credentials
```

## Deploying an ApolloAgent CRD

Once the operator is running, create agent resources declaratively:

```yaml
apiVersion: apollo.dev/v1
kind: ApolloAgent
metadata:
  name: my-crawler
  namespace: apollo
spec:
  agentSource: "https://github.com/org/openclaw.git"
  tenantId: "user_123"
  replicas: 2
  secretKeyRef:
    name: apollo-credentials
    key: secret-keys
  env:
    LOG_LEVEL: info
  restartPolicy:
    maxRestarts: 3
    windowSecs: 60
  resources:
    cpu: "0.5"
    memory: "512mb"
    timeoutSecs: 120
```

```bash
kubectl apply -f my-crawler.yaml
kubectl get apolloagents -n apollo
```

## Values Reference

### Global

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `global.imageRegistry` | string | `""` | Optional registry prefix for all images |
| `global.imagePullSecrets` | list | `[]` | Pull secret names attached to every pod |
| `global.storageClass` | string | `""` | Default StorageClass (overridden per component) |

### Node

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `node.replicaCount` | int | `1` | Number of node pods (StatefulSet) |
| `node.image.repository` | string | `ghcr.io/elgrhydev/apollo-node` | Image repository |
| `node.image.tag` | string | `2.2.0` | Image tag |
| `node.image.pullPolicy` | string | `IfNotPresent` | Pull policy |
| `node.service.type` | string | `ClusterIP` | Service type |
| `node.service.port` | int | `8080` | Service port |
| `node.resources.requests.cpu` | string | `500m` | CPU request |
| `node.resources.requests.memory` | string | `512Mi` | Memory request |
| `node.resources.limits.cpu` | string | `2000m` | CPU limit |
| `node.resources.limits.memory` | string | `2Gi` | Memory limit |
| `node.persistence.enabled` | bool | `true` | Enable PVC for .apollo/ state |
| `node.persistence.size` | string | `20Gi` | PVC size |
| `node.persistence.storageClass` | string | `""` | StorageClass (falls back to global) |
| `node.persistence.accessMode` | string | `ReadWriteOnce` | PVC access mode |
| `node.persistence.existingClaim` | string | `""` | Use an existing PVC |
| `node.secretKeys` | string | `""` | Comma-separated API keys (prefer secretKeysSecretRef) |
| `node.secretKeysSecretRef` | object | `nil` | External secret ref: `{name, key}` |
| `node.jwtSecret` | string | `""` | JWT signing secret (prefer jwtSecretRef) |
| `node.jwtSecretRef` | object | `nil` | External secret ref: `{name, key}` |
| `node.region` | string | `us-east-1` | Node region label |
| `node.webhookUrl` | string | `""` | Lifecycle event webhook URL |
| `node.tls.enabled` | bool | `false` | Enable TLS on 8443 |
| `node.tls.secretName` | string | `""` | K8s TLS Secret name (cert-manager etc.) |

### Hub

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `hub.replicaCount` | int | `1` | Number of hub pods |
| `hub.image.repository` | string | `ghcr.io/elgrhydev/apollo-hub` | Image repository |
| `hub.image.tag` | string | `2.2.0` | Image tag |
| `hub.service.type` | string | `ClusterIP` | Service type |
| `hub.service.port` | int | `9191` | Service port |
| `hub.resources.requests.cpu` | string | `200m` | CPU request |
| `hub.resources.limits.memory` | string | `512Mi` | Memory limit |
| `hub.hubKey` | string | `""` | Hub API key (prefer hubKeySecretRef) |
| `hub.hubKeySecretRef` | object | `nil` | External secret ref: `{name, key}` |
| `hub.scaleThreshold` | string | `0.80` | Fleet utilisation threshold for auto-scale webhook |
| `hub.webhookUrl` | string | `""` | Auto-scale webhook URL |

### Operator

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `operator.enabled` | bool | `true` | Deploy the operator |
| `operator.image.repository` | string | `ghcr.io/elgrhydev/apollo-operator` | Image repository |
| `operator.image.tag` | string | `2.2.0` | Image tag |
| `operator.resources.limits.memory` | string | `256Mi` | Memory limit |
| `operator.watchNamespace` | string | `""` | Namespace to watch (empty = all) |

### ServiceAccount

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `serviceAccount.create` | bool | `true` | Create a ServiceAccount |
| `serviceAccount.name` | string | `""` | Override generated name |
| `serviceAccount.annotations` | object | `{}` | SA annotations (e.g. IRSA) |

### Ingress

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `ingress.enabled` | bool | `false` | Enable Ingress |
| `ingress.className` | string | `nginx` | IngressClass name |
| `ingress.annotations` | object | `{}` | Ingress annotations |
| `ingress.hosts` | list | `[...]` | Host rules; each path has a `service` field: `node` or `hub` |
| `ingress.tls` | list | `[...]` | TLS termination secrets |

### Autoscaling

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `autoscaling.enabled` | bool | `false` | Enable HPA for node |
| `autoscaling.minReplicas` | int | `1` | Minimum replicas |
| `autoscaling.maxReplicas` | int | `10` | Maximum replicas |
| `autoscaling.targetCPUUtilizationPercentage` | int | `70` | CPU utilisation target |
| `autoscaling.targetMemoryUtilizationPercentage` | int | `80` | Memory utilisation target |

## Architecture Notes

- **StatefulSet for node**: each pod gets a dedicated PVC under `apollo-data-<pod>`. This means pod-0's state is isolated from pod-1's. For shared state across replicas, use a ReadWriteMany StorageClass (e.g., EFS on AWS, Filestore on GCP) and set `node.persistence.accessMode=ReadWriteMany`.
- **Hub polls nodes every 10 s**: the `APOLLO_NODE_ADDR` env var points to the node ClusterIP service. The hub discovers individual pods via the StatefulSet headless service DNS if needed.
- **Operator RBAC**: the ClusterRole grants the operator watch/patch access to ApolloAgent CRDs, Deployments, Services, Secrets (read-only), and PVCs cluster-wide. Scope it to a namespace by setting `operator.watchNamespace`.
- **CRD lifecycle**: the CRD is installed with `helm.sh/hook: pre-install,pre-upgrade` and `helm.sh/resource-policy: keep`, so it survives `helm uninstall`.

## Troubleshooting

```bash
# Check node logs
kubectl logs -n apollo -l app.kubernetes.io/component=node --tail=100

# Check operator logs
kubectl logs -n apollo -l app.kubernetes.io/component=operator --tail=100

# Describe a failing pod
kubectl describe pod -n apollo -l app.kubernetes.io/component=node

# Verify secret exists
kubectl get secret -n apollo apollo-credentials -o yaml

# Check ApolloAgent events
kubectl describe apolloagent my-crawler -n apollo
```
