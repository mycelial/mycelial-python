.PHONY: all
all:

.PHONY: dev
dev:
	cargo-watch  -i 'Makefile*' -i '4913*' -i '*.ipynb' --why -s 'maturin build'
