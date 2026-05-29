{{/*
Expand the name of the chart.
*/}}
{{- define "apollo.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
We truncate at 63 chars because some Kubernetes name fields are limited to this
(by the DNS naming spec).
*/}}
{{- define "apollo.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "apollo.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels applied to every object.
*/}}
{{- define "apollo.labels" -}}
helm.sh/chart: {{ include "apollo.chart" . }}
{{ include "apollo.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels (stable subset used in matchLabels / Services).
*/}}
{{- define "apollo.selectorLabels" -}}
app.kubernetes.io/name: {{ include "apollo.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Component-scoped selector labels.
Usage: include "apollo.componentSelectorLabels" (dict "root" . "component" "node")
*/}}
{{- define "apollo.componentSelectorLabels" -}}
app.kubernetes.io/name: {{ include "apollo.name" .root }}
app.kubernetes.io/instance: {{ .root.Release.Name }}
app.kubernetes.io/component: {{ .component }}
{{- end }}

{{/*
Component-scoped labels (full set).
*/}}
{{- define "apollo.componentLabels" -}}
helm.sh/chart: {{ include "apollo.chart" .root }}
{{ include "apollo.componentSelectorLabels" . }}
{{- if .root.Chart.AppVersion }}
app.kubernetes.io/version: {{ .root.Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .root.Release.Service }}
{{- end }}

{{/*
ServiceAccount name.
*/}}
{{- define "apollo.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "apollo.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Resolve the image registry prefix (trailing slash already included when set).
*/}}
{{- define "apollo.imageRegistry" -}}
{{- if .Values.global.imageRegistry -}}
{{- .Values.global.imageRegistry | trimSuffix "/" | printf "%s/" -}}
{{- end -}}
{{- end }}

{{/*
Render imagePullSecrets from global.imagePullSecrets list.
*/}}
{{- define "apollo.imagePullSecrets" -}}
{{- with .Values.global.imagePullSecrets }}
imagePullSecrets:
  {{- range . }}
  - name: {{ . }}
  {{- end }}
{{- end }}
{{- end }}

{{/*
Resolve the effective StorageClass for persistence.
Priority: componentStorageClass > global.storageClass > cluster default ("")

Call as: include "apollo.storageClass" (dict "root" . "storageClass" .Values.node.persistence.storageClass)
*/}}
{{- define "apollo.storageClass" -}}
{{- $sc := "" -}}
{{- if .root.Values.global.storageClass -}}
  {{- $sc = .root.Values.global.storageClass -}}
{{- end -}}
{{- if .storageClass -}}
  {{- $sc = .storageClass -}}
{{- end -}}
{{- if $sc -}}
storageClassName: {{ $sc | quote }}
{{- else -}}
storageClassName: ""
{{- end -}}
{{- end }}

{{/*
Name of the Secret that holds sensitive credentials.
*/}}
{{- define "apollo.secretName" -}}
{{- include "apollo.fullname" . }}-credentials
{{- end }}

{{/*
Full name for the node StatefulSet / Service.
*/}}
{{- define "apollo.node.fullname" -}}
{{- include "apollo.fullname" . }}-node
{{- end }}

{{/*
Full name for the hub Deployment / Service.
*/}}
{{- define "apollo.hub.fullname" -}}
{{- include "apollo.fullname" . }}-hub
{{- end }}

{{/*
Full name for the operator Deployment.
*/}}
{{- define "apollo.operator.fullname" -}}
{{- include "apollo.fullname" . }}-operator
{{- end }}
