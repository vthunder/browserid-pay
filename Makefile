# browserid-pay
#
#   make build / test        cargo, always through `ssh localtest` (Gatekeeper
#                            stalls test binaries launched from a normal shell)
#   make release-pay         release HEAD's custodian image to dokku by image
#                            (CI builds it; this is the manual release until the
#                            browserid-pay-ci deploy key is authorized on the host)
#   make dev-pay             run the custodian locally in dev mode on :3300

SHA  := $(shell git rev-parse HEAD)
REG  := ghcr.io/vthunder/browserid-pay
HOST ?= dokku@sandmill.org
SSH  := ssh -i $(HOME)/.ssh/mini-ops -o IdentitiesOnly=yes -o StrictHostKeyChecking=accept-new
LT   := ssh localtest 'cd ~/src/browserid-pay &&

.PHONY: build test release-pay watch dev-pay

build:
	$(LT) cargo build --workspace 2>&1 | sed "s/\x1b\[[0-9;]*m//g" | grep -E "^(error|warning)" -A5; true'

test:
	$(LT) cargo test --workspace 2>&1 | sed "s/\x1b\[[0-9;]*m//g" | grep -E "^error|FAILED|panicked|^test result"'

watch:
	gh run watch -R vthunder/browserid-pay --exit-status $$(gh run list -R vthunder/browserid-pay -L1 --json databaseId -q '.[0].databaseId')

release-pay:
	-$(SSH) $(HOST) git:from-image pay $(REG)/custodian:$(SHA)

dev-pay:
	PAY_DEV=1 PAY_PORT=3300 ADMIN_TOKEN=dev DATABASE_PATH=/tmp/custodian-dev.db \
	  $(LT) cargo run -p custodian'
