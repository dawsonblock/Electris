.PHONY: build test run clean docker verify

# Build the project
build:
	cargo build --release

# Run tests
test:
	cargo test --workspace

# Run the server locally
run:
	cargo run --release

# Clean build artifacts
clean:
	cargo clean

# Build Docker image
docker:
	docker build -t spine-server .

# Run verification script
verify:
	./scripts/verify.sh

# Run with docker-compose
up:
	docker-compose up -d

# Stop docker-compose
down:
	docker-compose down

# Format code
fmt:
	cargo fmt --all

# Run clippy
lint:
	cargo clippy --workspace -- -D warnings
