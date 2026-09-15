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

.PHONY: build test release-pay release-index watch dev-pay demo-local

build:
	$(LT) cargo build --workspace 2>&1 | sed "s/\x1b\[[0-9;]*m//g" | grep -E "^(error|warning)" -A5; true'

test:
	$(LT) cargo test --workspace 2>&1 | sed "s/\x1b\[[0-9;]*m//g" | grep -E "^error|FAILED|panicked|^test result"'

watch:
	gh run watch -R vthunder/browserid-pay --exit-status $$(gh run list -R vthunder/browserid-pay -L1 --json databaseId -q '.[0].databaseId')

release-pay:
	-$(SSH) $(HOST) git:from-image pay $(REG)/custodian:$(SHA)

release-index:
	-$(SSH) $(HOST) git:from-image index $(REG)/indexer:$(SHA)

# Local end-to-end demo / test (demo/README.md): builds the custodian, judge
# and demo-idp, then runs two contracts (settle; dispute + ruling) against
# them with a stub daemon. Exits non-zero on any failed assertion.
demo-local:
	$(LT) cargo build -p custodian -p judge -p demo-idp 2>&1 | sed "s/\x1b\[[0-9;]*m//g" | grep -E "^error" -A5; test $${PIPESTATUS[0]} -eq 0'
	$(LT) cd demo && (test -d node_modules || npm install --silent) && node run-local.mjs'

dev-pay:
	PAY_DEV=1 PAY_PORT=3300 ADMIN_TOKEN=dev DATABASE_PATH=/tmp/custodian-dev.db \
	  $(LT) cargo run -p custodian'

# ---- chain genesis (crates/genesis, deploy/GENESIS.md) -----------------------
#   make genesis-build      build genesis.wire from ~/secure-backup/dsp-sys.key
#   make genesis-submit     submit genesis.wire to Avail turing via TurboDA
#   make genesis-status ID=<submission-id>
#   make genesis-identity BLOCK=<n>   canonical identity, _sbo record, daemon entry
#   make genesis-verify     the daemon serves app 530's root policy
#   make dnssec-refresh     refresh /sys/dnssec/browserid.me on 530 if stale (cron)
# Secrets from ./.env (gitignored): AVAIL_NETWORK, AVAIL_APP_ID, AVAIL_TURBODA_KEY.
GENESIS := $(HOME)/.cache/cargo-target/debug/dsp-genesis
DAEMON  ?= https://da.sandmill.org
APP_ID  ?= 530
ENV     := set -a; . ./.env; set +a;

.PHONY: build-genesis genesis-build genesis-submit genesis-status genesis-identity genesis-verify dnssec-refresh

build-genesis:
	$(LT) cargo build -p dsp-genesis 2>&1 | sed "s/\x1b\[[0-9;]*m//g" | grep -E "^(error|warning)|-->|Finished"'

genesis-build: build-genesis
	$(GENESIS) build --daemon $(DAEMON)

genesis-submit: build-genesis
	$(ENV) $(GENESIS) submit --file genesis.wire

genesis-status: build-genesis
	@test -n "$(ID)" || { echo "usage: make genesis-status ID=<submission-id>"; exit 1; }
	$(ENV) $(GENESIS) status $(ID)

genesis-identity: build-genesis
	@test -n "$(BLOCK)" || { echo "usage: make genesis-identity BLOCK=<block>"; exit 1; }
	$(GENESIS) identity --block $(BLOCK) --app-id $(APP_ID)

# The daemon verifies the genesis itself against expected_genesis when it syncs
# past B ("Genesis verified" in `dokku logs sbo-daemon`); this checks the
# observable consequence — the root policy is readable from the 530 database.
genesis-verify:
	@curl -sf "$(DAEMON)/v1/object?repo=sbo%2Braw%3A%2F%2Favail%3Aturing%3A$(APP_ID)%2F&path=/sys/policies/&id=root" >/dev/null \
	  && echo "ok: /sys/policies/root readable from app $(APP_ID) via $(DAEMON)" \
	  || { echo "not yet: daemon does not serve app $(APP_ID) (deploy mingo deploy/sbo-daemon, then wait for sync past 3885219)"; exit 1; }

dnssec-refresh: build-genesis
	$(ENV) $(GENESIS) dnssec-refresh --daemon $(DAEMON) browserid.me

release-judge:
	-$(SSH) $(HOST) git:from-image judge $(REG)/judge:$(SHA)
