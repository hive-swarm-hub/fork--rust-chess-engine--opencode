#!/usr/bin/env bash
# download_engines.sh
#
# Downloads open-source chess engines spanning the CCRL Blitz ELO range,
# plus the UHO Lichess opening book.
#
# All download URLs are pinned to verified releases with confirmed Linux
# x86-64 binaries. CCRL Blitz (2'+1") ratings taken from the March 2026
# rating list. Engines marked (8CPU) were tested with 8 threads on CCRL;
# they will perform slightly lower in a single-threaded tournament.
#
# Requirements: curl, unzip, tar
# Output:
#   tournament/engines/<name>   – executable
#   tournament/books/ccrl.epd   – opening book

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINES_DIR="$SCRIPT_DIR/engines"
BOOKS_DIR="$SCRIPT_DIR/books"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$ENGINES_DIR" "$BOOKS_DIR"

# ── helpers ──────────────────────────────────────────────────────────────────

install_bare() {
    # Direct binary download (no archive)
    local name="$1" url="$2"
    echo "  [$name] $url"
    curl -fsSL "$url" -o "$ENGINES_DIR/$name"
    chmod +x "$ENGINES_DIR/$name"
}

install_tar() {
    # .tar or .tar.gz – extract and find the binary by name pattern
    local name="$1" url="$2" pattern="${3:-$name}"
    echo "  [$name] $url"
    local file="$TMP/${name}.tar"
    curl -fsSL "$url" -o "$file"
    mkdir -p "$TMP/${name}_extract"
    # handle both .tar and .tar.gz
    if [[ "$url" == *.tar.gz || "$url" == *.tgz ]]; then
        tar -xzf "$file" -C "$TMP/${name}_extract"
    else
        tar -xf "$file" -C "$TMP/${name}_extract"
    fi
    local bin
    bin=$(find "$TMP/${name}_extract" -type f -name "${pattern}*" | head -1)
    cp "$bin" "$ENGINES_DIR/$name"
    chmod +x "$ENGINES_DIR/$name"
}

install_zip() {
    # .zip archive – find the binary inside by name pattern
    local name="$1" url="$2" pattern="${3:-$name}"
    echo "  [$name] $url"
    local file="$TMP/${name}.zip"
    curl -fsSL "$url" -o "$file"
    mkdir -p "$TMP/${name}_extract"
    unzip -q "$file" -d "$TMP/${name}_extract"
    local bin
    bin=$(find "$TMP/${name}_extract" -type f \( -executable -o -name "${pattern}*" \) | grep -v '\.dll\|\.txt\|\.md' | head -1)
    cp "$bin" "$ENGINES_DIR/$name"
    chmod +x "$ENGINES_DIR/$name"
}

skip() {
    echo "  [SKIP] $1 – $2"
}

# ── engines ───────────────────────────────────────────────────────────────────
# Format: install_<type> <local-name> <url>
# CCRL Blitz ELO noted in comments (March 2026 list, 1CPU unless noted)

echo "================================================================"
echo "  Downloading engines"
echo "================================================================"

# ── Tier 1: ~3750-3800 ────────────────────────────────────────────────────────

# Stockfish 18  |  ~3792 CCRL (8CPU)  |  ships as a .tar with one binary inside
install_tar  stockfish \
    "https://github.com/official-stockfish/Stockfish/releases/download/sf_18/stockfish-ubuntu-x86-64-avx2.tar" \
    "stockfish"

# Stormphrax 7.0.0  |  ~3750 CCRL (8CPU)
install_bare stormphrax \
    "https://github.com/Ciekce/Stormphrax/releases/download/v7.0.0/stormphrax-7.0.0-avx2-bmi2"

# ── Tier 2: ~3620-3750 ────────────────────────────────────────────────────────

# Viridithas 19.0.1  |  3742 CCRL (1CPU)
install_bare viridithas \
    "https://github.com/cosmobobak/viridithas/releases/download/v19.0.1/viridithas-19.0.1-linux-x86-64-v3"

# Tcheran 11.0  |  3634 CCRL (1CPU)
install_bare tcheran \
    "https://github.com/jgilchrist/tcheran/releases/download/v11.0/tcheran-v11.0-linux-x86_64-v3"

# Koivisto 9.0  |  3689 CCRL (8CPU)
install_bare koivisto \
    "https://github.com/Luecx/Koivisto/releases/download/v9.0/Koivisto_9.0-linux-avx2-pgo"

# Black Marlin 9.0  |  3629 CCRL (8CPU)
install_bare blackmarlin \
    "https://github.com/jnlt3/blackmarlin/releases/download/9.0/blackmarlin-linux-x86-64-v3"

# akimbo 1.0.0  |  3621 CCRL (8CPU)
install_bare akimbo \
    "https://github.com/jw1912/akimbo/releases/download/v1.0.0/akimbo-1.0.0-avx2"

# ── Tier 3: ~3390-3545 ────────────────────────────────────────────────────────

# Patricia 5.0  |  3542 CCRL (1CPU)
install_bare patricia \
    "https://github.com/Adam-Kulju/Patricia/releases/download/5/patricia_v3"

# Carp 3.0.1  |  3529 CCRL (1CPU)
install_bare carp \
    "https://github.com/dede1751/carp/releases/download/v3.0.1/carp-v3.0.1-linux-x86_64-V4"

# BlackCore 6.0  |  3444 CCRL (1CPU)
install_bare blackcore \
    "https://github.com/SzilBalazs/BlackCore/releases/download/6.0/BlackCore-avx2-linux"

# Avalanche 2.1.0  |  3396 CCRL (1CPU)  |  v3=avx2; v4 requires AVX-512
install_bare avalanche \
    "https://github.com/SnowballSH/Avalanche/releases/download/v2.1.0/Avalanche-2.1.0-x86_64-linux-v3"

# ── Tier 4: ~3060-3370 ────────────────────────────────────────────────────────

# Frozenight 6.0.0  |  3367 CCRL (1CPU)
install_bare frozenight \
    "https://github.com/MinusKelvin/frozenight/releases/download/v6.0.0/frozenight-6.0.0-linux-x86-64-v3"

# Nalwald 19  |  3346 CCRL (1CPU)
install_bare nalwald \
    "https://github.com/tsoj/Nalwald/releases/download/19/Nalwald-19-linux-amd64-modern"

# Wahoo 4.0.0  |  3085 CCRL (1CPU)
install_bare wahoo \
    "https://github.com/spamdrew128/Wahoo/releases/download/4.0.0/wahoo_v4-x86_64-linux-v4"

# 4ku 5.1  |  3061 CCRL (1CPU)
install_bare 4ku \
    "https://github.com/kz04px/4ku/releases/download/v5.1/4ku-5.1-avx2"

# Inanis 1.6.0  |  3084 CCRL (1CPU)  |  ships as .zip
# Inanis 1.6.0  |  3084 CCRL (1CPU)  |  ships as .zip; explicitly pick the 'inanis' binary
echo "  [inanis] https://github.com/Tearth/Inanis/releases/download/v1.6.0/inanis_1.6.0_linux_64bit_x86-64_popcnt_bmi2.zip"
curl -fsSL "https://github.com/Tearth/Inanis/releases/download/v1.6.0/inanis_1.6.0_linux_64bit_x86-64_popcnt_bmi2.zip" \
    -o "$TMP/inanis.zip"
mkdir -p "$TMP/inanis_extract"
unzip -q "$TMP/inanis.zip" -d "$TMP/inanis_extract"
cp "$TMP/inanis_extract/inanis" "$ENGINES_DIR/inanis"
chmod +x "$ENGINES_DIR/inanis"

# ── Tier 5: ~2550 ─────────────────────────────────────────────────────────────

# Tantabus 2.0.0  |  2553 CCRL (1CPU)
install_bare tantabus \
    "https://github.com/analog-hors/tantabus/releases/download/v2.0.0/tantabus-ubuntu-20.04-x86-64-v3"

# ── opening book ──────────────────────────────────────────────────────────────

echo ""
echo "================================================================"
echo "  Downloading opening book (UHO Lichess 4852)"
echo "================================================================"
curl -fsSL \
    "https://raw.githubusercontent.com/official-stockfish/books/master/UHO_Lichess_4852_v1.epd.zip" \
    -o "$TMP/book.zip"
unzip -q "$TMP/book.zip" -d "$TMP/book"
find "$TMP/book" -name "*.epd" | head -1 | xargs -I{} cp {} "$BOOKS_DIR/ccrl.epd"
echo "  Installed -> $BOOKS_DIR/ccrl.epd"

# ── summary ──────────────────────────────────────────────────────────────────

echo ""
echo "================================================================"
echo "  Installed engines"
echo "================================================================"
printf "  %-20s  %s\n" "Name" "Path"
printf "  %-20s  %s\n" "----" "----"
for f in "$ENGINES_DIR"/*; do
    [[ -x "$f" ]] && printf "  %-20s  %s\n" "$(basename "$f")" "$f"
done
echo ""
echo "  Book: $(wc -l < "$BOOKS_DIR/ccrl.epd") positions in $BOOKS_DIR/ccrl.epd"
echo ""
echo "Done. Run ./run_tournament.sh to start the tournament."
