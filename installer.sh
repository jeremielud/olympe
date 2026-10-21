#!/bin/sh
# Compile Olympe et l'installe pour l'utilisateur courant (Linux et autres Unix) :
# le programme dans ~/.local/bin, l'entrée de menu et l'icône dans ~/.local/share.
# Les données du jeu s'installent ensuite avec « olympe installer <source> ».
set -eu
cd "$(dirname "$0")"
prefix="${PREFIX:-$HOME/.local}"

command -v cargo >/dev/null || { echo "Il faut Rust (cargo) : https://www.rust-lang.org/fr/tools/install" >&2; exit 1; }
cargo build --release --bin olympe

install -Dm755 target/release/olympe "$prefix/bin/olympe"
install -Dm644 paquet/olympe.desktop "$prefix/share/applications/olympe.desktop"
install -Dm644 paquet/olympe.svg "$prefix/share/icons/hicolor/scalable/apps/olympe.svg"

echo "Olympe est installé dans $prefix/bin."
case ":$PATH:" in
    *":$prefix/bin:"*) ;;
    *) echo "Ajoutez $prefix/bin à votre PATH pour le lancer par son nom." ;;
esac
echo "Installez ensuite les données de votre exemplaire du jeu :"
echo "    olympe installer <dossier du jeu, ou images des CD de Zeus puis de Poséidon>"
