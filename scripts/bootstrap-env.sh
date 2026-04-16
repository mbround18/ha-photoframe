#!/usr/bin/env bash
set -euo pipefail

ESP_IDF_PYTHON_ENV_PATH="${HOME}/.espressif/python_env/idf5.5_py3.12_env"
ESP_IDF_PYTHON_BIN="${ESP_IDF_PYTHON_ENV_PATH}/bin/python"

if ! command -v uv >/dev/null 2>&1; then
  echo "uv is required but was not found in PATH." >&2
  exit 1
fi

mkdir -p "$(dirname "${ESP_IDF_PYTHON_ENV_PATH}")"

echo "Bootstrapping ESP-IDF Python environment at ${ESP_IDF_PYTHON_ENV_PATH}"
uv venv --seed --clear -p /usr/bin/python3 "${ESP_IDF_PYTHON_ENV_PATH}"

echo "Verifying Python environment"
"${ESP_IDF_PYTHON_BIN}" --version
"${ESP_IDF_PYTHON_BIN}" -m pip --version

cat <<'EOF'

Next steps:
  cargo firmware-check
  cargo firmware-build
EOF