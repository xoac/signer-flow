# Guard for running just without args. just list recipes
recipes-list:
    just --list

# Check code and README.md updates
check:
    cargo clippy --all-targets --all-features --workspace -- -D warnings
    cargo fmt --all -- --check

# Check and test code
test: check
    cargo test

# Clean and update dependencies in Cargo.lock
clean-and-update:
    cargo clean
    cargo update

# Try push changes to origin but check for typical errors
test-and-push: test
    cargo deny check
    git push

# Check and push (use test-and-push) instead!
check-and-push: check
    cargo deny check
    git push

# Load version from Cargo.toml
version := `sed -En 's/version[[:space:]]*=[[:space:]]*"([^"]+)"/\1/p' Cargo.toml | head -1`

# create version tag and push to origin
tag-version: clean-and-update test
    cargo deny check
    grep -Fq '[{{ version }}]' CHANGELOG.md                  # The CHANGELOG.md should contains updated changes
    git diff --no-ext-diff --quiet --exit-code              # All files should be committed
    git tag -a {{ version }} -m "Release {{ version }}"
    git push origin {{ version }}

# crate tag push it to origin and then publish to crates.io
tag-and-publish: tag-version
    cargo publish

# build application from Dockerfile and push it
minikube-update-app:
  docker build -t signer-rest-api .
  kubectl delete -f state-producer.yaml 
  sleep 45
  minikube image rm signer-rest-api
  minikube image load signer-rest-api
  kubectl apply -f state-producer.yaml
