MAKEFLAGS := --no-builtin-rules --no-print-directory

# Note the lazy assignment, in contrast to other variables:
VERSION = $(shell cargo pkgid | awk -F'#' '{print $$2}')

.PHONY: default
default: checks

# =============================================================================
# dev
# =============================================================================

.PHONY: clean
clean:
	cargo clean
	rm -rf bin

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
	git tag v$(VERSION)
