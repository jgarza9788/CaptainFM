# Sourced by record-demo03.sh after demo-fixture.sh: adds a Showcase folder (an image, a source file and a video for the
# preview scene) to the synthetic home in $H. The big archive for the search and speed scenes is made by make-big-archive.py.
mkdir -p "$H/Showcase"
magick -size 1600x1000 plasma:'#0b1d2e-#1fa6a0' -blur 0x6 -modulate 105,120 "$H/Showcase/aurora.jpg"
cp "$H/Projects/homelab/docker-compose.yml" "$H/Showcase/compose.yml"
printf 'fn main() {\n    let helm = Helm::new("home");\n    for file in helm.files() {\n        println!("{}", file.name());\n    }\n}\n' > "$H/Showcase/main.rs"
ffmpeg -loglevel error -y -f lavfi -i "gradients=s=1280x720:rate=30:duration=8:c0=0b1d2e:c1=1fa6a0:c2=c9a24b:c3=0b1d2e:nb_colors=4:speed=0.04" -c:v libx264 -pix_fmt yuv420p "$H/Showcase/voyage.mp4"
for f in "$H/Showcase"/*; do touch -d "2026-10-1$((RANDOM % 9)) 1$((RANDOM % 9)):$((RANDOM % 5))$((RANDOM % 9))" "$f"; done
