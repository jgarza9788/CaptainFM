#!/usr/bin/env bash
# Builds ~/Videos/CaptainFM_trailer02.mp4 (1080p, ~45 s, 120 BPM, cuts land on the beat) from the live scenes that
# scripts/record-demo02.sh records (DEMO folder), with ffmpeg only.
set -euo pipefail
cd "$(dirname "$0")/.."
DEMO=${DEMO:-$HOME/Videos/cfm-demo02}
OUT=${1:-$HOME/Videos/CaptainFM_trailer02.mp4}
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
B=/usr/share/fonts/TTF/JetBrainsMonoNerdFont-Bold.ttf
BG=0x0b0d12; ACC=0x7aa2f7; HOT=0xff4d6d
FPS=30
IDX=0
next() { IDX=$((IDX+1)); NAME="$W/$(printf %02d "$IDX").mp4"; }
frames() { awk "BEGIN{print int($1*$FPS)}"; }

# word <seconds> <bg> <fg> <text> <size>: one slammed word on a flat color, hard cut.
word() {
  next
  ffmpeg -v error -y -f lavfi -i "color=c=$2:s=1920x1080:r=$FPS:d=$1" -vf "\
drawtext=fontfile=$B:text='$4':fontsize='$5+40*max(0,1-t*8)':fontcolor=$3:x=(w-text_w)/2:y=(h-text_h)/2,\
format=yuv420p" "$NAME"
}

# shot <image> <seconds> <headline> [key] [in|out]: full-bleed screenshot, punch zoom, flash cut, slide-in caption.
shot() {
  next
  local n=$1 d=$2 h=$3 k=${4:-} dir=${5:-in} f; f=$(frames "$2")
  local z="1+0.30*on/$f"; [ "$dir" = out ] && z="1.30-0.30*on/$f"
  local keybox=""
  [ -n "$k" ] && keybox=",drawtext=fontfile=$B:text=' $k ':fontsize=72:fontcolor=black:box=1:boxcolor=$ACC:boxborderw=26:x=w-text_w-90:y=70"
  ffmpeg -v error -y -i "docs/images/$n.png" -vf "\
scale=3840:-1:flags=lanczos,crop=3840:2160:0:(ih-2160)/2,\
zoompan=z='$z':x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':d=$f:s=1920x1080:fps=$FPS,\
drawbox=x=0:y=780:w=1920:h=300:color=black@0.62:t=fill,\
drawbox=x=0:y=780:w=1920:h=6:color=$ACC:t=fill,\
drawtext=fontfile=$B:text='$h':fontsize=104:fontcolor=white:x='(w-text_w)/2+if(lt(t,0.3),(0.3-t)*-5000,0)':y=845\
$keybox,\
fade=t=in:st=0:d=0.2:color=white,format=yuv420p" "$NAME"
}

# wide <image> <seconds> <headline>: for the wide themes strip.
wide() {
  next
  local f; f=$(frames "$2")
  ffmpeg -v error -y -i "docs/images/$1.png" -vf "\
scale=3840:-1:flags=lanczos,zoompan=z='1+0.10*on/$f':x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':d=$f:s=1920x389:fps=$FPS,\
pad=1920:1080:0:420:color=$BG,\
drawtext=fontfile=$B:text='$3':fontsize=104:fontcolor=white:x='(w-text_w)/2+if(lt(t,0.3),(0.3-t)*-5000,0)':y=170,\
drawbox=x=0:y=820:w=1920:h=6:color=$ACC:t=fill,\
fade=t=in:st=0:d=0.2:color=white,format=yuv420p" "$NAME"
}

# clip <scene> <seconds-into-actions> <source-seconds> <out-seconds> <headline>: a slice of a recorded scene, sped up to fit,
# with the caption band. Times count from where the scene's actions begin ($DEMO/<scene>.t plus an offset set per scene).
clip() {
  next
  local sc=$1 start; start=$(awk "BEGIN{print $(cat "$DEMO/$1.t")+$2}")
  ffmpeg -v error -y -ss "$start" -t "$3" -i "$DEMO/$sc.mp4" -vf "\
setpts=PTS*$4/$3,fps=$FPS,scale=1920:1080:flags=lanczos,\
drawbox=x=0:y=860:w=1920:h=220:color=black@0.62:t=fill,\
drawbox=x=0:y=860:w=1920:h=6:color=$ACC:t=fill,\
drawtext=fontfile=$B:text='$5':fontsize=96:fontcolor=white:x='(w-text_w)/2+if(lt(t,0.3),(0.3-t)*-5000,0)':y=905,\
fade=t=in:st=0:d=0.2:color=white,format=yuv420p" -t "$4" "$NAME"
}

# Cold open: slammed words.
word 0.5 $BG   white 'DRAG.'       200
word 0.5 $ACC  black 'HOVER.'      200
word 0.5 $BG   white 'DROP.'       200
word 0.5 $HOT  black 'LIKE A PRO.' 170
word 1.5 $BG   white 'CAPTAINFM 2' 130

# Drag and drop, live. Pointer scenes start with a second of nothing, then the drag.
clip drop        1.2 4.0 4   'LIVE DROP LABELS'
clip drop        5.2 5.8 4   'CTRL COPIES. ALT LINKS.'
clip hover       1.2 9.5 6   'HOVER TO OPEN FOLDERS'
clip miller-drag 1.2 6.0 4   'DRAG BETWEEN COLUMNS'

# The rest of the app.
clip browse      0.6 3.5 3   'BROWSE AT KEY SPEED'
clip tree        0.6 3.2 3   'TREE. MILLER. ANY VIEW.'
clip miller      0.6 3.2 3   'MILLER COLUMNS'
clip grid        0.6 3.0 3   'THUMBNAILS'
clip rename      0.6 3.5 3   'BATCH RENAME'
clip search      0.6 4.0 3   'SEARCH EVERYTHING'
clip palette     0.6 3.0 3   'EVERYTHING IS A COMMAND'
clip t-gruvbox   0.6 2.0 1   'WEARS YOUR THEME'
clip t-rose      0.6 2.0 1   'WEARS YOUR THEME'
clip t-latte     0.6 2.0 2   'WEARS YOUR THEME'
word 0.5 $BG   white 'RUST.'       200
word 0.5 $ACC  black 'QT 6.'       200
word 1.0 $HOT  black '10 MB.'      240

# End card with the new anchor.
rsvg-convert -w 300 -h 300 packaging/captainfm.svg -o "$W/icon.png"
next
ffmpeg -v error -y -f lavfi -i "color=c=$BG:s=1920x1080:r=$FPS:d=3" -loop 1 -i "$W/icon.png" -filter_complex "\
[0]drawtext=fontfile=$B:text='CAPTAINFM':fontsize='140+30*max(0,1-t*6)':fontcolor=white:x=(w-text_w)/2:y=470,\
drawtext=fontfile=$B:text='TAKE THE HELM.':fontsize=60:fontcolor=$ACC:x=(w-text_w)/2:y=680:alpha='min(1,max(0,t-0.4)*4)',\
drawtext=fontfile=$B:text='github.com/jgarza9788/CaptainFM':fontsize=34:fontcolor=0x9aa5b1:x=(w-text_w)/2:y=860:alpha='min(1,max(0,t-0.8)*4)'[t];\
[t][1:v]overlay=x=(W-w)/2:y=110:shortest=1,fade=t=out:st=2.4:d=0.6,format=yuv420p" -t 3 "$NAME"

for f in "$W"/*.mp4; do echo "file '$f'"; done > "$W/list"
ffmpeg -v error -y -f concat -safe 0 -i "$W/list" -c copy "$W/video.mp4"
D=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$W/video.mp4")

# Score, 120 BPM: four-on-the-floor kick, off-beat hats, driving A-F-C-G bass, 16th-note arp, rising swell.
# tb = time within the beat, tbar = time within a 2 s bar.
BASS="if(lt(mod(t,8),2),55,if(lt(mod(t,8),4),43.65,if(lt(mod(t,8),6),65.41,49)))"
ARP="if(lt(mod(t,8),2),440,if(lt(mod(t,8),4),349.23,if(lt(mod(t,8),6),523.25,392)))*if(lt(mod(t*8,4),1),1,if(lt(mod(t*8,4),2),1.5,if(lt(mod(t*8,4),3),2,1.5)))"
EXPR="0.55*sin(2*PI*(45*mod(t,0.5)+5*(1-exp(-30*mod(t,0.5)))))*exp(-9*mod(t,0.5))\
+0.14*(random(0)*2-1)*exp(-70*mod(t+0.25,0.5))\
+0.30*sin(2*PI*$BASS*t)*(0.5+0.5*lt(mod(t,0.25),0.18))*(1-0.6*exp(-6*mod(t,0.5)))\
+0.07*sin(2*PI*($ARP)*t)*exp(-10*mod(t,0.125))"
ffmpeg -v error -y -f lavfi -i "aevalsrc='$EXPR':s=44100:d=$D" \
  -af "afade=t=in:d=0.1,afade=t=out:st=$(awk "BEGIN{print $D-1.2}"):d=1.2,alimiter=limit=0.9,volume=1.4" "$W/audio.wav"
ffmpeg -v error -y -i "$W/video.mp4" -i "$W/audio.wav" -c:v libx264 -crf 19 -preset slow -c:a aac -b:a 192k -shortest -movflags +faststart "$OUT"
echo "wrote $OUT ($D s)"
