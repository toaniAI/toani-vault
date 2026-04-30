#!/bin/sh

set -eu

NAMESPACE="${1:?namespace is required}"
DEPLOYMENT="${2:?deployment name is required}"
SERVICE_NAME="${3:?service name is required}"
PORT="${4:-8080}"
TARGET_PORT="${5:-http}"
READINESS_PATH="${READINESS_PATH:-/ready}"
LIVENESS_PATH="${LIVENESS_PATH:-/health}"
PROBE_TIMEOUT_SECONDS="${PROBE_TIMEOUT_SECONDS:-2}"
TERMINATION_GRACE_PERIOD_SECONDS="${TERMINATION_GRACE_PERIOD_SECONDS:-45}"
PRE_STOP_SLEEP_SECONDS="${PRE_STOP_SLEEP_SECONDS:-20}"

selector_yaml="$(kubectl -n "$NAMESPACE" get deployment "$DEPLOYMENT" -o go-template='{{range $k, $v := .spec.selector.matchLabels}}{{printf "    %s: %s\n" $k $v}}{{end}}')"

if [ -z "$selector_yaml" ]; then
    echo "failed to resolve selector labels from deployment/$DEPLOYMENT in namespace/$NAMESPACE" >&2
    exit 1
fi

cat <<EOF | kubectl apply -f -
apiVersion: v1
kind: Service
metadata:
  name: ${SERVICE_NAME}
  namespace: ${NAMESPACE}
spec:
  clusterIP: None
  publishNotReadyAddresses: true
  selector:
${selector_yaml}
  ports:
    - name: http
      port: ${PORT}
      targetPort: ${TARGET_PORT}
      protocol: TCP
EOF

kubectl -n "$NAMESPACE" patch deployment "$DEPLOYMENT" --type=strategic -p "
{
  \"spec\": {
    \"template\": {
      \"spec\": {
        \"terminationGracePeriodSeconds\": ${TERMINATION_GRACE_PERIOD_SECONDS},
        \"containers\": [
          {
            \"name\": \"go\",
            \"lifecycle\": {
              \"preStop\": {
                \"exec\": {
                  \"command\": [\"/bin/sh\", \"-c\", \"sleep ${PRE_STOP_SLEEP_SECONDS}\"]
                }
              }
            },
            \"livenessProbe\": {
              \"httpGet\": {\"path\": \"${LIVENESS_PATH}\", \"port\": \"${TARGET_PORT}\"},
              \"initialDelaySeconds\": 10,
              \"periodSeconds\": 10,
              \"timeoutSeconds\": ${PROBE_TIMEOUT_SECONDS},
              \"failureThreshold\": 3
            },
            \"readinessProbe\": {
              \"httpGet\": {\"path\": \"${READINESS_PATH}\", \"port\": \"${TARGET_PORT}\"},
              \"initialDelaySeconds\": 5,
              \"periodSeconds\": 5,
              \"timeoutSeconds\": ${PROBE_TIMEOUT_SECONDS},
              \"failureThreshold\": 3
            }
          }
        ]
      }
    }
  }
}"
