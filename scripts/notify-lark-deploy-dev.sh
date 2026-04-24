#!/bin/sh

set -eu

commit_message="$(printf '%s' "${DRONE_COMMIT_MESSAGE:-}" | tr '\r\n' '  ')"
text="$(printf '%s\n' \
  '部署' \
  'CredBridge dev 部署成功' \
  "仓库: ${DRONE_REPO}" \
  "分支: ${DRONE_BRANCH}" \
  "构建号: ${DRONE_BUILD_NUMBER}" \
  "提交: ${DRONE_COMMIT_SHA}" \
  "提交说明: ${commit_message}" \
  "链接: ${DRONE_BUILD_LINK}")"

jq -Rn \
  --arg text "$text" \
  '{msg_type:"text",content:{text:$text}}' \
  | curl -fsSL -X POST -H "Content-Type: application/json" \
      --data-binary @- \
      https://open.larksuite.com/open-apis/bot/v2/hook/7c903754-0db9-46bc-a363-03184c024bdb
