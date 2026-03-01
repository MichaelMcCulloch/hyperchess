.PHONY: profile profile2d profile3d profile4d build-profile

PROFILE_BIN = ./target/release/bench_profile
PROFILES_DIR = profiles
CONCURRENCY = 1
MINUTES = 10

build-profile:
	RUSTFLAGS="-C target-cpu=native" cargo build --release --bin bench_profile

$(PROFILES_DIR):
	mkdir -p $(PROFILES_DIR)

profile2d: build-profile $(PROFILES_DIR)
	HYPERCHESS_COMPUTE_MINUTES=$(MINUTES) HYPERCHESS_COMPUTE_CONCURRENCY=$(CONCURRENCY) \
		perf record -g -F 997 -o $(PROFILES_DIR)/perf_2d.data -- $(PROFILE_BIN) 2 10
	perf script -i $(PROFILES_DIR)/perf_2d.data | inferno-collapse-perf | \
		inferno-flamegraph --title "HyperChess 2D (depth 10)" > $(PROFILES_DIR)/flamegraph_2d.svg
	perf report -i $(PROFILES_DIR)/perf_2d.data --stdio --no-children --percent-limit 1.0

profile3d: build-profile $(PROFILES_DIR)
	HYPERCHESS_COMPUTE_MINUTES=$(MINUTES) HYPERCHESS_COMPUTE_CONCURRENCY=$(CONCURRENCY) \
		perf record -g -F 997 -o $(PROFILES_DIR)/perf_3d.data -- $(PROFILE_BIN) 3 5
	perf script -i $(PROFILES_DIR)/perf_3d.data | inferno-collapse-perf | \
		inferno-flamegraph --title "HyperChess 3D (depth 5)" > $(PROFILES_DIR)/flamegraph_3d.svg
	perf report -i $(PROFILES_DIR)/perf_3d.data --stdio --no-children --percent-limit 1.0

profile4d: build-profile $(PROFILES_DIR)
	HYPERCHESS_COMPUTE_MINUTES=$(MINUTES) HYPERCHESS_COMPUTE_CONCURRENCY=$(CONCURRENCY) \
		perf record -g -F 997 -o $(PROFILES_DIR)/perf_4d.data -- $(PROFILE_BIN) 4 3
	perf script -i $(PROFILES_DIR)/perf_4d.data | inferno-collapse-perf | \
		inferno-flamegraph --title "HyperChess 4D (depth 3)" > $(PROFILES_DIR)/flamegraph_4d.svg
	perf report -i $(PROFILES_DIR)/perf_4d.data --stdio --no-children --percent-limit 1.0

profile: profile2d profile3d profile4d
