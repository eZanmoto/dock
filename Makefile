# Note that `target` is used as the output directory for Rust so care should be
# taken that collisions don't occur between Rust output and local output.
tgt_dir:=target
tgt_test_dir:=$(tgt_dir)/tests

test_org:=ezanmoto
test_proj:=dock
test_base_img_name:=$(test_org)/$(test_proj).test_base
test_base_img_tag:=latest
test_base_img:=$(test_base_img_name):$(test_base_img_tag)
test_img_namespace:=$(test_org)/$(test_proj).test
test_vol_namespace:=$(test_org).$(test_proj).test

.PHONY: check
check: \
		check_intg \
		check_lint

# We pull base Docker images required by the tests, even though they'd
# automatically be pulled during builds, in order to make the output more
# predictable.
#
# We run `clean_images` before starting the tests in order to make the tests
# more deterministic, because having leftover images from previous runs can
# cause the output from `docker build` to be altered (due to the use of
# caching).
.PHONY: check_intg
check_intg: clean_images $(tgt_test_dir)
	bash scripts/docker_rbuild.sh \
			$(test_base_img_name) \
			$(test_base_img_tag) \
			- \
			< test_base.Dockerfile
	TEST_IMG_NAMESPACE='$(test_img_namespace)' \
		TEST_DIR='$(shell pwd)/$(tgt_test_dir)' \
		TEST_BASE_IMG='$(test_base_img)' \
		TEST_ORG='$(test_org)' \
		TEST_PROJ='$(test_proj).test' \
		cargo test \
			-- \
			--show-output \
			$(TESTS)
	# Descendents of the test base image indicate that `dock` didn't clean up
	# after all operations. See "Test Base Image" in `tests/cli/README.md` for
	# more information.
	bash scripts/check_no_descendents.sh '$(test_base_img)'

.PHONY: check_lint
check_lint:
	TEST_IMG_NAMESPACE='$(test_img_namespace)' \
		TEST_DIR='$(shell pwd)/$(tgt_test_dir)' \
		TEST_BASE_IMG='$(test_base_img)' \
		TEST_ORG='$(test_org)' \
		TEST_PROJ='$(test_proj).test' \
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

.PHONY: clean
clean: clean_images

.PHONY: clean_images
clean_images:
	bash scripts/clean_images.sh \
		'$(test_img_namespace)'

.PHONY: clean_volumes
clean_volumes:
	bash scripts/clean_volumes.sh \
		'$(test_vol_namespace)'
