# lueur

Synchronise des lampes Philips Hue avec les films lus sur Jellyfin, depuis un serveur Linux (Umbrel), sans PC Windows, sans Hue Sync et sans Sync Box.

## Principe

1. **Analyse, une fois par film** : ffmpeg décode la vidéo (VA-API/Quick Sync sur un Intel N100) et la réduit à 96×54 sur le GPU. lueur ignore les bandes noires et enregistre une grille de 16×9 couleurs, 12 fois par seconde, dans un fichier `.lueur` compressé (quelques centaines de Ko pour un film de 2 h).
2. **Lecture** : lueur interroge Jellyfin toutes les 500 ms, extrapole la position entre deux rapports et garde une horloge locale verrouillée (recalage seulement sur avance rapide, pause ou changement de média ; petites dérives corrigées en douceur).
3. **Lampes** : chaque canal de la zone de divertissement prend la couleur de la partie de l'écran qui correspond à sa position, envoyée à 50 Hz via l'API Hue Entertainment (DTLS 1.2 PSK, UDP 2100). À l'arrêt, le flux est coupé et les lampes reprennent leur état.

Au moment du film, la charge est quasi nulle : aucun décodage, juste une lecture dans la piste.

## Installation

Il faut Rust, ffmpeg et OpenSSL. Sur macOS :

```sh
brew install rust ffmpeg openssl@3
git clone https://github.com/mathieuletyrant/lueur && cd lueur
OPENSSL_DIR="$(brew --prefix openssl@3)" cargo build --release
```

Le binaire est dans `target/release/lueur`. Sur macOS, l'analyse décode avec VideoToolbox.

## Utilisation

```sh
# 1. Appairer (appuyer sur le bouton du pont)
lueur pair 192.168.68.xx
# 2. Voir les zones de divertissement et leurs canaux
lueur areas
# 3. Vérifier avec une mire arc-en-ciel
lueur test --seconds 10
# 4. Analyser la bibliothèque (ignore les films déjà à jour)
lueur analyze /downloads/movies /downloads/shows -j 2
# 5. Suivre Jellyfin
lueur run
```

`lueur inspect piste.lueur --at 754` affiche la grille à un instant donné dans le terminal.

## Configuration (`lueur.json`)

```json
{
  "store": "/data/tracks",
  "hue": { "bridge": "192.168.68.xx", "username": "…", "clientkey": "…", "area": "Salon TV" },
  "jellyfin": { "url": "http://192.168.68.58:8096", "api_key": "…", "device": "Apple TV" },
  "path_map": [{ "from": "/media", "to": "/downloads" }],
  "look": { "brightness": 1.0, "saturation": 1.15, "min_brightness": 0.0, "smoothing_ms": 60, "offset_ms": 0 },
  "auto_analyze": true
}
```

- `path_map` : chemins vus par Jellyfin → chemins vus par lueur.
- `jellyfin.device` : ne suivre qu'un appareil (sous-chaîne du nom ou du client).
- `look.offset_ms` : si les lampes changent en retard sur les coupes, augmenter (ex. 500).
- `auto_analyze` : un film lu sans piste est analysé en arrière-plan.
- La clé API Jellyfin doit être une clé admin (pour obtenir le chemin du fichier).

## Tests

`cargo test` couvre l'extraction (bandes noires, grille), le format de piste, le paquet HueStream, la correspondance canaux/écran, l'horloge et le parsing des dates Jellyfin. Le prototype a aussi été testé de bout en bout contre un faux pont (REST + `openssl s_server` en DTLS PSK) et un faux Jellyfin.

## À faire

- Websocket Jellyfin au lieu du sondage, webhook Radarr/Sonarr à l'import pour analyser automatiquement.
- Interface web (réglages en direct, décalage depuis le canapé, aperçu de la piste).
- Empaquetage en app Umbrel (`mathieu-lueur`).
