#!/bin/sh
# Records the site's GIFs and stills from our own app, offscreen (no window opens).
#   sh website/gen/make_gifs.sh SCRATCH_DIR
# Needs a release build of shapecraft-snap, gifski and ffmpeg (brew install gifski ffmpeg). Takes about 3 minutes.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
work=${1:?scratch dir}
snap=${SNAP:-$repo/target/release/shapecraft-snap}
media=$repo/website/media
mkdir -p "$work/scn" "$media"
python3 "$here/gifs.py" "$work/scn" >/dev/null
for name in sketch extrude fillet rotate; do
  rm -rf "$work/$name"; mkdir -p "$work/$name"
  "$snap" "$work/scn/$name.jsonl" "$work/$name" >/dev/null
  if [ "$name" = rotate ]; then
    # The camera moves in every frame of this one, so a video is a fifth of the size of the GIF.
    ffmpeg -loglevel error -y -framerate 15 -i "$work/$name/f%04d.png" -vf "scale=920:-2,format=yuv420p" \
      -c:v libx264 -crf 24 -preset slow -movflags +faststart -an "$media/$name.mp4"
    ffmpeg -loglevel error -y -i "$work/$name/f0040.png" -vf scale=920:-1 -q:v 4 "$media/$name.jpg"
    echo "$media/$name.mp4"
    continue
  fi
  gifski --quiet --fps 15 --width 920 --quality 90 -o "$media/$name.gif" "$work/$name"/f*.png
  echo "$media/$name.gif"
done
# The speed-lapse: a video for the page (much smaller than a GIF this long), a GIF for the README, a poster.
rm -rf "$work/handheld"; mkdir -p "$work/handheld"
"$snap" "$work/scn/handheld.jsonl" "$work/handheld" >/dev/null
ffmpeg -loglevel error -y -framerate 15 -i "$work/handheld/f%04d.png" -vf "scale=1380:-2,format=yuv420p" \
  -c:v libx264 -crf 22 -preset slow -movflags +faststart -an "$media/handheld.mp4"
gifski --quiet --fps 15 --width 720 --quality 80 -o "$media/handheld.gif" "$work/handheld"/f*.png
last=$(ls "$work/handheld"/f*.png | tail -1)
ffmpeg -loglevel error -y -i "$last" -vf scale=1380:-1 -q:v 3 "$media/handheld.jpg"
echo "$media/handheld.mp4"
rm -rf "$work/stills"; mkdir -p "$work/stills"
"$snap" "$work/scn/stills.jsonl" "$work/stills" >/dev/null
for f in "$work/stills"/*.png; do
  b=$(basename "$f" .png)
  ffmpeg -loglevel error -y -i "$f" -vf scale=1380:-1 -q:v 3 "$media/$b.jpg"
  echo "$media/$b.jpg"
done
