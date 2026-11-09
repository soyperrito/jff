PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
MANDIR ?= $(PREFIX)/share/man/man1
BASHCOMPDIR ?= $(PREFIX)/share/bash-completion/completions
ZSHCOMPDIR ?= $(PREFIX)/share/zsh/site-functions
FISHCOMPDIR ?= $(PREFIX)/share/fish/vendor_completions.d

CARGO ?= cargo
BIN := target/release/jff

.PHONY: all build release test fmt clippy completions man install uninstall clean

all: build

build:
	$(CARGO) build --release --locked

release: build completions man

test:
	$(CARGO) test --all-targets

fmt:
	$(CARGO) fmt --all -- --check

clippy:
	$(CARGO) clippy --all-targets -- -D warnings

completions: build
	mkdir -p completions
	$(BIN) completions bash > completions/jff.bash
	$(BIN) completions zsh > completions/_jff
	$(BIN) completions fish > completions/jff.fish

man: build
	mkdir -p man
	$(BIN) man > man/jff.1

install: build
	mkdir -p man completions
	$(BIN) man > man/jff.1
	$(BIN) completions bash > completions/jff.bash
	$(BIN) completions zsh > completions/_jff
	$(BIN) completions fish > completions/jff.fish
	mkdir -p "$(DESTDIR)$(BINDIR)" "$(DESTDIR)$(MANDIR)" "$(DESTDIR)$(BASHCOMPDIR)" "$(DESTDIR)$(ZSHCOMPDIR)" "$(DESTDIR)$(FISHCOMPDIR)"
	install -m 755 $(BIN) "$(DESTDIR)$(BINDIR)/jff"
	install -m 644 man/jff.1 "$(DESTDIR)$(MANDIR)/jff.1"
	install -m 644 completions/jff.bash "$(DESTDIR)$(BASHCOMPDIR)/jff"
	install -m 644 completions/_jff "$(DESTDIR)$(ZSHCOMPDIR)/_jff"
	install -m 644 completions/jff.fish "$(DESTDIR)$(FISHCOMPDIR)/jff.fish"

uninstall:
	rm -f "$(DESTDIR)$(BINDIR)/jff"
	rm -f "$(DESTDIR)$(MANDIR)/jff.1"
	rm -f "$(DESTDIR)$(BASHCOMPDIR)/jff"
	rm -f "$(DESTDIR)$(ZSHCOMPDIR)/_jff"
	rm -f "$(DESTDIR)$(FISHCOMPDIR)/jff.fish"

clean:
	$(CARGO) clean
	rm -rf completions man
