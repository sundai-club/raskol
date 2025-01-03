MAKEFLAGS := --no-builtin-rules --no-print-directory

# Note the lazy assignment to avoid running commands when they aren't needed:
PKG_VERSION = $(shell cargo pkgid | awk -F'#' '{print $$2}')
PKG_NAME    = $(shell cargo pkgid | awk -F'#' '{print $$1}' | awk -F / '{print $$NF}')

REL_TARGET   := x86_64-unknown-linux-musl
REL_DIR_PATH := ./target/$(REL_TARGET)/release
REL_BIN       = $(REL_DIR_PATH)/$(PKG_NAME)

# TODO DNS
SRV_ADDR          := 35.234.250.103
SRV_ADMIN_USER    := admin
SRV_SERVICE_USER  := raskol
SRV_SERVICE_GROUP := $(SRV_SERVICE_USER)
SRV_SERVICE_DIR   := /opt/raskol
SRV_SERVICE_NAME  := raskol.service
SRV_SERVICE_PATH  := /etc/systemd/system/$(SRV_SERVICE_NAME)

.PHONY: default
default: checks

# =============================================================================
# release
# =============================================================================

.PHONY: release_build
release_build:
	cargo build -p $(PKG_NAME) --release --target $(REL_TARGET)
	mkdir -p bin
	cp -f $(REL_BIN) bin/

.PHONY: release_push
release_push:
	rsync -avz --no-times --no-perms --no-owner --no-group --delete bin $(SRV_ADMIN_USER)@$(SRV_ADDR):$(SRV_SERVICE_DIR)/
	ssh $(SRV_ADMIN_USER)@$(SRV_ADDR) sudo mkdir -p $(SRV_SERVICE_DIR)/data
	ssh $(SRV_ADMIN_USER)@$(SRV_ADDR) sudo chown -R $(SRV_SERVICE_USER):$(SRV_SERVICE_GROUP) $(SRV_SERVICE_DIR)/bin
	ssh $(SRV_ADMIN_USER)@$(SRV_ADDR) sudo chmod -R 770   $(SRV_SERVICE_DIR)/bin
	scp sys$(SRV_SERVICE_PATH) $(SRV_ADMIN_USER)@$(SRV_ADDR):$(SRV_SERVICE_PATH)

.PHONY: release_reload
release_reload:
	ssh $(SRV_ADMIN_USER)@$(SRV_ADDR) sudo systemctl daemon-reload
	ssh $(SRV_ADMIN_USER)@$(SRV_ADDR) sudo systemctl enable $(SRV_SERVICE_NAME)

.PHONY: release_start
release_start:
	ssh $(SRV_ADMIN_USER)@$(SRV_ADDR) sudo systemctl start $(SRV_SERVICE_NAME)

.PHONY: release_stop
release_stop:
	ssh $(SRV_ADMIN_USER)@$(SRV_ADDR) sudo systemctl stop $(SRV_SERVICE_NAME)

.PHONY: release_restart
release_restart:
	ssh $(SRV_ADMIN_USER)@$(SRV_ADDR) sudo systemctl restart $(SRV_SERVICE_NAME)

.PHONY: release_status
release_status:
	ssh $(SRV_ADMIN_USER)@$(SRV_ADDR) sudo systemctl status $(SRV_SERVICE_NAME)

.PHONY: release_to_production
release_to_production:
	$(MAKE) release_build
	$(MAKE) release_stop
	$(MAKE) release_status || true  # XXX Stopped service status exits with non-zero.
	$(MAKE) release_push
	$(MAKE) release_reload
	$(MAKE) release_start
	$(MAKE) release_status

.PHONY: release_target_install
release_target_install:
	rustup target add $(REL_TARGET)

# =============================================================================
# dev
# =============================================================================

.PHONY: clean
clean:
	cargo clean
	rm bin/raskol

.PHONY: checks
checks:
	cargo check
	cargo test -- --nocapture
	cargo clippy -- \
		-W clippy::pedantic \
		-W clippy::cast-possible-truncation \
		-W clippy::cast-sign-loss \
		-A clippy::redundant_closure_for_method_calls \
		-A clippy::single_match_else \
		-A clippy::uninlined-format-args \
		-A clippy::missing_errors_doc
	cargo fmt --check

.PHONY: clippy_nursery
clippy_nursery:
	cargo clippy -- -W clippy::nursery

.PHONY: clippy_cargo
clippy_cargo:
	cargo clippy -- -W clippy::cargo

.PHONY: clippy_pedantic
clippy_pedantic:
	cargo clippy -- \
		-W clippy::pedantic \
		-A clippy::single_match_else \
		-A clippy::uninlined-format-args \
		-A clippy::missing_errors_doc

.PHONY: tag
tag:
	git tag v$(PKG_VERSION)

# =============================================================================
# test
# =============================================================================

CERT_CERT := cert/cert.pem
CERT_KEY := cert/key.pem

.PHONY: local_cert_files
local_cert_files: $(CERT_KEY) $(CERT_CERT)

$(CERT_KEY) $(CERT_CERT): | cert
	openssl \
		req -x509 \
		-newkey rsa:4096 \
		-keyout $(CERT_KEY) \
		-out $(CERT_CERT) \
		-days 365 \
		-nodes \
		-subj '/CN=localhost' \
		-addext 'subjectAltName=DNS:localhost,IP:127.0.0.1'

cert:
	mkdir -p $@
