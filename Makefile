FLASH_PORT ?= /dev/ttyUSB0
FLASH_BAUD ?= 460800
MONITOR_BAUD ?= 115200
FIRMWARE_TARGET := riscv32imafc-esp-espidf
HOST_TARGET := x86_64-unknown-linux-gnu
FIRMWARE_MANIFEST := packages/frame-firmware/Cargo.toml
FIRMWARE_BUILD_CMD := cargo build --manifest-path $(FIRMWARE_MANIFEST) --release --target $(FIRMWARE_TARGET) -Zbuild-std=std,panic_abort
ESP_IDF_VERSION := v5.5.3
ESP_IDF_TOOLS_DIR := $(HOME)/.espressif
ESP_IDF_PYTHON_ENV := $(ESP_IDF_TOOLS_DIR)/python_env/idf5.5_py3.12_env
ESP_IDF_PYTHON_BIN := $(ESP_IDF_PYTHON_ENV)/bin/python
RELEASE_BUILD_DIR := target/$(FIRMWARE_TARGET)/release/build
RELEASE_DIR := target/$(FIRMWARE_TARGET)/release
APP_ELF := $(RELEASE_DIR)/frame-firmware
APP_BIN := $(RELEASE_DIR)/frame-firmware.bin
BOOTLOADER_BIN := $(RELEASE_DIR)/bootloader.bin
PARTITION_TABLE_BIN := $(RELEASE_DIR)/partition-table.bin
BASE_PATH := /usr/bin:/bin:$(HOME)/.cargo/bin
SYSTEM_ENV := /usr/bin/env
FIRMWARE_BUILD_ENV := $(SYSTEM_ENV) -u VIRTUAL_ENV -u CONDA_PREFIX -u CONDA_DEFAULT_ENV -u PYTHONHOME -u PYTHONPATH -u UV_PROJECT_ENVIRONMENT PATH="$(BASE_PATH)" PYTHON=/usr/bin/python3 MCU=esp32p4 ESP_IDF_VERSION=$(ESP_IDF_VERSION) ESP_IDF_TOOLS_INSTALL_DIR=global ESP_IDF_SYS_ROOT_CRATE=frame-firmware ESP_IDF_SDKCONFIG_DEFAULTS="$(CURDIR)/sdkconfig.defaults" IDF_PYTHON_ENV_PATH="$(ESP_IDF_PYTHON_ENV)"
FIRMWARE_TOOL_ENV := $(SYSTEM_ENV) -u VIRTUAL_ENV -u CONDA_PREFIX -u CONDA_DEFAULT_ENV -u PYTHONHOME -u PYTHONPATH -u UV_PROJECT_ENVIRONMENT PATH="$(BASE_PATH):$(ESP_IDF_PYTHON_ENV)/bin" IDF_PYTHON_ENV_PATH="$(ESP_IDF_PYTHON_ENV)"

.PHONY: bootstrap-python-env clean-firmware-ui-cache build format lint flash monitor dev

bootstrap-python-env:
	@if [ ! -x "$(ESP_IDF_PYTHON_BIN)" ]; then \
		./scripts/bootstrap-env.sh; \
	fi

clean-firmware-ui-cache:
	@find target/$(FIRMWARE_TARGET) -maxdepth 3 -type d -name 'esp-idf-sys-*' -exec rm -rf {} + 2>/dev/null || true

build: bootstrap-python-env clean-firmware-ui-cache
	$(FIRMWARE_BUILD_ENV) $(FIRMWARE_BUILD_CMD)

format:
	cargo fmt --all

lint:
	$(FIRMWARE_BUILD_ENV) cargo clippy --workspace --all-targets --target $(HOST_TARGET) -- -D warnings

flash: build
	@set -e; \
	flash_args=$$(find target/$(FIRMWARE_TARGET) -type f -path '*/out/build/flash_project_args' -printf '%T@ %p\n' | sort -nr | head -n1 | cut -d' ' -f2-); \
	if [ -z "$$flash_args" ]; then \
		echo "Missing ESP-IDF flash bundle under target/$(FIRMWARE_TARGET). Run 'make build' first." >&2; \
		exit 1; \
	fi; \
	build_dir=$$(dirname "$$flash_args"); \
	flash_opts=$$(head -n1 "$$flash_args"); \
	app_bin="$(abspath $(APP_BIN))"; \
	tmp_args=$$(mktemp); \
	cleanup() { rm -f "$$tmp_args"; }; \
	trap cleanup EXIT; \
	echo "Generating app image $$app_bin from $(APP_ELF)"; \
	$(FIRMWARE_TOOL_ENV) $(ESP_IDF_PYTHON_BIN) -m esptool --chip esp32p4 elf2image $$flash_opts --output "$$app_bin" "$(abspath $(APP_ELF))"; \
	awk -v app_bin="$$app_bin" '{ if ($$2 == "libespidf.bin") $$2 = app_bin; print }' "$$flash_args" > "$$tmp_args"; \
	echo "Flashing ESP-IDF bundle from $$build_dir on $(FLASH_PORT)"; \
	cd "$$build_dir" && $(FIRMWARE_TOOL_ENV) $(ESP_IDF_PYTHON_BIN) -m esptool --chip esp32p4 --port $(FLASH_PORT) --baud $(FLASH_BAUD) write_flash @"$$tmp_args"

monitor:
	@echo "Opening serial monitor on $(FLASH_PORT) at $(MONITOR_BAUD) baud (exit with Ctrl+C)."; \
	$(FIRMWARE_TOOL_ENV) $(ESP_IDF_PYTHON_BIN) scripts/serial_monitor.py $(FLASH_PORT) $(MONITOR_BAUD)

dev:
	@$(MAKE) flash
	@$(MAKE) monitor