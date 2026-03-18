{{/*
CredBridge Config Chart Helper Templates
*/}}

{{/*
Expand the name of the chart.
*/}}
{{- define "credbridge-config.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "credbridge-config.fullname" -}}
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
{{- define "credbridge-config.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "credbridge-config.labels" -}}
helm.sh/chart: {{ include "credbridge-config.chart" . }}
{{ include "credbridge-config.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "credbridge-config.selectorLabels" -}}
app.kubernetes.io/name: {{ include "credbridge-config.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
ConfigMap name
*/}}
{{- define "credbridge-config.configMapName" -}}
{{- printf "%s-config" (include "credbridge-config.fullname" .) }}
{{- end }}

{{/*
ExternalSecret name
*/}}
{{- define "credbridge-config.externalSecretName" -}}
{{- printf "%s-vault-secrets" (include "credbridge-config.fullname" .) }}
{{- end }}
