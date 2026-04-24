#!/bin/sh

set -eu

NAMESPACE="${1:?namespace is required}"
DEPLOYMENT="${2:?deployment name is required}"
SERVICE_NAME="${3:?service name is required}"
PORT="${4:-8080}"
TARGET_PORT="${5:-http}"

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
${selector_yaml}  ports:
    - name: http
      port: ${PORT}
      targetPort: ${TARGET_PORT}
      protocol: TCP
EOF
