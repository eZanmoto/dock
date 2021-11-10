# Note that `target` is used as the output directory for Rust so care should be
# taken that collisions don't occur between Rust output and local output.
tgt_dir:=target
tgt_test_dir:=$(tgt_dir)/tests
test_imgs:=alpine:3.14.2
test_img_namespace:=ezanmoto/dock.test

.PHONY: check
check: \
	check_intg \
	check_lint

# We pull base Docker images required by the tests, even though they'd
# automatically be pulled during builds, in order to make the output more
# predictable.
.PHONY: check_intg
check_intg: $(tgt_test_dir)
	docker image inspect \
			$(test_imgs) \
			>/dev/null \
		|| docker pull \
			$(test_imgs)
	TEST_IMG_NAMESPACE='$(test_img_namespace)' \
		TEST_DIR='$(shell pwd)/$(tgt_test_dir)' \
		cargo test \
			-- \
			--show-output \
			$(TESTS)

.PHONY: check_lint
check_lint:
	TEST_IMG_NAMESPACE='$(test_img_namespace)' \
		TEST_DIR='$(shell pwd)/$(tgt_test_dir)' \
		cargo clippy \
			--all-targets \
			--all-features \
			-- \
			-D warnings \
			-D clippy::pedantic \
			-D clippy::cargo \
			-A clippy::module-name-repetitions
	python3 scripts/check_line_length.py \
			'**/*.rs' \
			79

# We tag `$(tgt_test_dir)` as phony so that the test directory is removed and
# recreated at the start of every test run.
.PHONY: $(tgt_test_dir)
$(tgt_test_dir): $(tgt_dir)
	rm -rf '$(tgt_test_dir)'
	mkdir '$@'

$(tgt_dir):
	mkdir '$@'
