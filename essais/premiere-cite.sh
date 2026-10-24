#!/bin/sh
# essais/premiere-cite.sh CAPTURE.png "étapes" : bâtit par script une cité pour le
# tutoriel « Construire une cité » (firstcity), puis joue les étapes données, par
# exemple "t:19200 bilan maisons" (deux ans, puis l'état de la cité et des maisons).
# Il faut un affichage : Xvfb et DISPLAY=:99 font l'affaire.
B="r:97,134,110,134 r:110,100,110,134 r:110,104,136,104 r:110,112,136,112 r:110,120,136,120 r:136,104,136,120"
B="$B b:112,102,Agora b:112,102,Epicière b:114,102,Marchand_de_laine b:116,102,Marchand_d'huile"
B="$B b:118,102,Fontaine b:120,102,Maintenance b:122,102,Hôtel_des_impôts b:124,102,Podium b:126,102,Fontaine b:130,101,Gymnase"
for x in 111 113 115 117 119 121 123 125 127 129 131 133; do B="$B b:$x,105,Logement_modeste"; done
for x in 111 113 115 117 119 121 123 125 127 129 131 133; do B="$B b:$x,107,Jardin_d'agrément"; done
for x in 111 113 115 117 119 121 123 125 127 129 131 133 134; do B="$B b:$x,109,Parc"; done
B="$B b:123,110,Agora b:123,110,Epicière b:121,110,Fontaine b:113,110,Maintenance b:131,110,Podium"
for x in 111 115 117 119 129 133; do B="$B b:$x,110,Logement_modeste"; done
for x in 111 113 115 117 119 121 123 125 127 129 131 133; do B="$B b:$x,113,Logement_modeste"; done
B="$B b:111,116,Grenier b:115,118,Maintenance b:117,117,Collège b:127,118,Pressoir_à_olives b:129,118,Maintenance b:125,118,Pressoir_à_olives"
B="$B b:111,121,Entrepôt b:114,121,Bergerie b:116,121,Métairie b:119,121,Pêcherie b:123,121,Pêcherie"
cd "$(dirname "$0")/.."
OLYMPE_MUET=1 ./target/release/olympe --sans-intro --ticks 0 --capture "$1" --script "$B $2" donnees/adventures/firstcity.pak 2>&1 | grep -v "^sauvegarde\|^après 0\|^script terminé" 
