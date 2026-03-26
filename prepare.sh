#!/usr/bin/env bash
# Install Rust toolchain, Stockfish, and cutechess-cli. Run once.
set -euo pipefail

cd "$(dirname "$0")"

TOOLS_DIR="tools"
mkdir -p "$TOOLS_DIR"

# --- Rust toolchain ---
if ! command -v cargo &>/dev/null; then
    echo "Installing Rust toolchain..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
else
    echo "Rust already installed: $(rustc --version)"
fi

# --- Stockfish ---
if [ ! -f "$TOOLS_DIR/stockfish" ]; then
    echo "Downloading Stockfish..."
    ARCH=$(uname -m)
    OS=$(uname -s)

    if [ "$OS" = "Linux" ]; then
        SF_URL="https://github.com/official-stockfish/Stockfish/releases/latest/download/stockfish-ubuntu-x86-64-avx2.tar"
        curl -sL "$SF_URL" -o /tmp/stockfish.tar
        tar xf /tmp/stockfish.tar -C /tmp/
        find /tmp/stockfish* -name "stockfish" -type f -exec cp {} "$TOOLS_DIR/stockfish" \;
        rm -rf /tmp/stockfish*
    elif [ "$OS" = "Darwin" ]; then
        if command -v brew &>/dev/null; then
            brew install stockfish 2>/dev/null || true
            SF_BIN=$(which stockfish 2>/dev/null || echo "")
            if [ -n "$SF_BIN" ]; then
                ln -sf "$SF_BIN" "$TOOLS_DIR/stockfish"
            fi
        fi
    fi

    if [ ! -f "$TOOLS_DIR/stockfish" ] && [ ! -L "$TOOLS_DIR/stockfish" ]; then
        echo "ERROR: Could not install Stockfish. Please install manually and place at $TOOLS_DIR/stockfish"
        exit 1
    fi
    chmod +x "$TOOLS_DIR/stockfish"
    echo "Stockfish installed."
else
    echo "Stockfish already installed."
fi

# --- Tournament manager: fastchess (preferred) or cutechess-cli (fallback) ---
# fastchess is easier to build (no Qt dependency) and has compatible CLI syntax.
if [ ! -f "$TOOLS_DIR/fastchess" ] && [ ! -f "$TOOLS_DIR/cutechess-cli" ]; then
    echo "Installing tournament manager..."
    OS=$(uname -s)

    # Try fastchess first (build from source, no dependencies)
    echo "  Building fastchess from source..."
    FASTCHESS_TMP=$(mktemp -d)
    if git clone --depth 1 https://github.com/Disservin/fastchess.git "$FASTCHESS_TMP/fastchess" 2>/dev/null; then
        if make -C "$FASTCHESS_TMP/fastchess" -j$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2) 2>/dev/null; then
            cp "$FASTCHESS_TMP/fastchess/fastchess" "$TOOLS_DIR/fastchess"
            chmod +x "$TOOLS_DIR/fastchess"
            echo "  fastchess installed."
        fi
    fi
    rm -rf "$FASTCHESS_TMP"

    # If fastchess failed, try cutechess-cli
    if [ ! -f "$TOOLS_DIR/fastchess" ]; then
        echo "  fastchess build failed, trying cutechess-cli..."
        if [ "$OS" = "Linux" ]; then
            if command -v apt-get &>/dev/null; then
                sudo apt-get install -y cutechess 2>/dev/null || true
            fi
            CC_BIN=$(which cutechess-cli 2>/dev/null || echo "")
            if [ -n "$CC_BIN" ]; then
                ln -sf "$CC_BIN" "$TOOLS_DIR/cutechess-cli"
            else
                CC_URL="https://github.com/cutechess/cutechess/releases/download/v1.3.1/cutechess-cli-1.3.1-linux64.tar.gz"
                curl -sL "$CC_URL" -o /tmp/cutechess.tar.gz 2>/dev/null || true
                if [ -f /tmp/cutechess.tar.gz ]; then
                    tar xzf /tmp/cutechess.tar.gz -C /tmp/
                    find /tmp/ -name "cutechess-cli" -type f -exec cp {} "$TOOLS_DIR/cutechess-cli" \;
                    rm -rf /tmp/cutechess*
                fi
            fi
        elif [ "$OS" = "Darwin" ]; then
            if command -v brew &>/dev/null; then
                brew install cutechess 2>/dev/null || true
                CC_BIN=$(which cutechess-cli 2>/dev/null || echo "")
                if [ -n "$CC_BIN" ]; then
                    ln -sf "$CC_BIN" "$TOOLS_DIR/cutechess-cli"
                fi
            fi
        fi
    fi

    if [ ! -f "$TOOLS_DIR/fastchess" ] && [ ! -f "$TOOLS_DIR/cutechess-cli" ] && [ ! -L "$TOOLS_DIR/cutechess-cli" ]; then
        echo "ERROR: Could not install fastchess or cutechess-cli."
        echo "Install manually: https://github.com/Disservin/fastchess or https://github.com/cutechess/cutechess"
        exit 1
    fi
    echo "Tournament manager ready."
else
    echo "Tournament manager already installed."
fi

# --- coreutils (macOS: provides gtimeout) ---
if [ "$(uname -s)" = "Darwin" ]; then
    if ! command -v timeout &>/dev/null && ! command -v gtimeout &>/dev/null; then
        echo "Installing coreutils (for gtimeout on macOS)..."
        if command -v brew &>/dev/null; then
            brew install coreutils 2>/dev/null || true
        fi
    fi
fi

# --- CCRL-rated reference engines ---
# Real engines with verified CCRL Blitz ratings for accurate ELO estimation.
# These provide diverse playing styles (not just Stockfish at limited strength).
echo "Downloading CCRL-rated reference engines..."

# Helper: download and install an engine binary
install_engine() {
    local name="$1" url="$2" extract_cmd="$3"
    local dest="$TOOLS_DIR/$name"
    if [ -f "$dest" ]; then
        echo "  $name already installed."
        return 0
    fi
    echo "  Downloading $name..."
    local tmpdir=$(mktemp -d)
    if curl -sL "$url" -o "$tmpdir/download" 2>/dev/null; then
        eval "$extract_cmd"
        if [ -f "$dest" ]; then
            chmod +x "$dest"
            echo "  $name installed."
        else
            echo "  WARNING: Failed to extract $name."
        fi
    else
        echo "  WARNING: Failed to download $name."
    fi
    rm -rf "$tmpdir"
}

# Blunder 6.1.0 — CCRL Blitz ~2105
install_engine "blunder-6" \
    "https://github.com/deanmchris/blunder/releases/download/v6.1.0/blunder-6.1.0.zip" \
    "cd \$tmpdir && unzip -q download && find . -path '*/linux/blunder' -exec cp {} $TOOLS_DIR/blunder-6 \;"

# Blunder 8.5.5 — CCRL Blitz ~2667
install_engine "blunder-8" \
    "https://github.com/deanmchris/blunder/releases/download/v8.5.5/blunder-8.5.5.zip" \
    "cd \$tmpdir && unzip -q download && find . -path '*/linux/blunder*default*' -exec cp {} $TOOLS_DIR/blunder-8 \; || find . -path '*/linux/blunder*' -type f -exec cp {} $TOOLS_DIR/blunder-8 \;"

# Inanis 1.6.0 — CCRL Blitz ~3085
install_engine "inanis" \
    "https://github.com/Tearth/Inanis/releases/download/v1.6.0/inanis_1.6.0_linux_64bit_x86-64_popcnt.zip" \
    "cd \$tmpdir && unzip -q download && find . -name 'inanis*' -type f -exec cp {} $TOOLS_DIR/inanis \;"

# Mantissa 3.7.2 — CCRL Blitz ~3317 (direct binary)
install_engine "mantissa" \
    "https://github.com/jtheardw/mantissa/releases/download/v3.7.2/mantissa-3.7.2-linux-avx2" \
    "cp \$tmpdir/download $TOOLS_DIR/mantissa"

# Stormphrax 7.0.0 — CCRL Blitz ~3723 (direct binary, aspirational ceiling)
install_engine "stormphrax" \
    "https://github.com/Ciekce/Stormphrax/releases/download/v7.0.0/stormphrax-7.0.0-avx2" \
    "cp \$tmpdir/download $TOOLS_DIR/stormphrax"

# --- Record tool checksums (anti-tampering) ---
echo "Recording tool checksums..."
CHECKSUM_FILE="$TOOLS_DIR/.checksums"
> "$CHECKSUM_FILE"
for tool in "$TOOLS_DIR"/*; do
    [ -f "$tool" ] || continue
    [[ "$(basename "$tool")" == .* ]] && continue
    if [ ! -L "$tool" ]; then
        shasum -a 256 "$tool" >> "$CHECKSUM_FILE"
    else
        real_path=$(readlink -f "$tool")
        hash=$(shasum -a 256 "$real_path" | awk '{print $1}')
        echo "$hash $tool" >> "$CHECKSUM_FILE"
    fi
done
echo "Checksums saved to $CHECKSUM_FILE"

# --- Python deps ---
echo "Installing Python dependencies..."
pip install -r requirements.txt 2>/dev/null || pip3 install -r requirements.txt

# --- Generate or download opening book ---
mkdir -p data
echo "Setting up opening book..."
BOOK_URL="https://raw.githubusercontent.com/official-stockfish/books/master/Drawkiller_balanced_big.epd.zip"
if curl -sL "$BOOK_URL" -o data/openings.epd.zip 2>/dev/null; then
    unzip -o data/openings.epd.zip -d data/ >/dev/null 2>&1
    mv data/Drawkiller_balanced_big.epd data/openings.epd 2>/dev/null || true
    rm -f data/openings.epd.zip
    echo "  Downloaded Drawkiller_balanced_big.epd (15k+ positions)."
else
    echo "  WARNING: Could not download opening book. Generating local fallback..."
    python3 gen_openings.py > data/openings.epd 2>&1 || {
        echo "  WARNING: Could not generate opening book. Using built-in fallback (30 positions)."
    }
fi
if [ -f "data/openings.epd" ]; then
    COUNT=$(wc -l < data/openings.epd | tr -d ' ')
    echo "Opening book: $COUNT positions in data/openings.epd"
fi

# --- Compile initial engine ---
echo "Compiling starter engine..."
cd engine
cargo build --release
cd ..

if [ ! -f "engine/target/release/hive-chess" ]; then
    echo "ERROR: Engine binary not found after compilation."
    exit 1
fi

echo ""
echo "Setup complete:"
echo "  Stockfish: $TOOLS_DIR/stockfish"
echo "  cutechess-cli: $TOOLS_DIR/cutechess-cli"
echo "  Engine: engine/target/release/hive-chess"
echo ""
echo "Run 'bash eval/eval.sh' to start evaluation."
