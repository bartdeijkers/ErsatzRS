Q: 
most efficiënt mpeg-ts bitrate and buffer size for streaming 2 digital signals over 100mbps ethernet network to make sure hls segmenter work smooth 

A:
To ensure smooth HLS segmenting over a 100Mbps Ethernet network, you need a balance that maximizes visual quality while preventing network congestion and buffer underruns at the segmenter level.

Because you are streaming **two** signals over a shared 100Mbps link, your absolute maximum combined burst bandwidth should never exceed 80% of the network capacity (**80Mbps total, or 40Mbps per signal**). However, for HLS segmentation, lower bitrates with predictable buffering provide much better stability.

Here is the optimal breakdown for configuring your MPEG-TS streams.

---

## 1. Bitrate Configuration (Per Signal)

For a stable 100Mbps local network, targeting **1080p60** or **1080i50/60** high-quality feeds is ideal.

* **Target Video Bitrate:** **12 Mbps to 15 Mbps** (H.264) or **6 Mbps to 8 Mbps** (HEVC/H.265).
* **Audio Bitrate:** **192 kbps to 256 kbps** (AAC or MP2).
* **MPEG-TS Muxrate (Total TS Bitrate):** Set a **CBR (Constant Bitrate)** or **Strict VBR** muxrate of **18 Mbps** per stream.

### Why CBR?

HLS segmenters slice files based on Keyframes (GOP boundaries). If you use unconstrained VBR, a sudden complex scene (like sports or flashing lights) can cause a bitrate spike that delays network transmission or overloads the segmenter’s input buffer, causing dropped frames in the resulting `.ts` or `.m4s` segments.

---

## 2. Buffer Size & VBV Settings

The Video Buffer Verifier (VBV) controls the peak bitrate fluctuations. For HLS segmenting, you want a relatively tight buffer to minimize latency and ensure the segmenter receives chunks at a predictable pace.

* **VBV Buffer Size (`bufsize`):** Set this equal to **1x to 2x the target bitrate**.
* *Example at 15 Mbps:* Set `bufsize` to **15 Mbits** (1-second buffer) or **30 Mbits** (2-second buffer).


* **VBV Max Bitrate (`maxrate`):** Match this strictly to your target bitrate if using CBR, or allow a maximum of **1.1x** the target for strict VBR.

> **Crucial for HLS:** Do not use massive buffers (e.g., 5+ seconds). A large buffer allows the encoder to dump a massive spike of data over the network all at once, which defeats the purpose of smoothing the input for the HLS segmenter.

---

## 3. The Core Factor for Smooth HLS: GOP & Alignment

The absolute bitrates matter less to an HLS segmenter than **GOP (Group of Pictures) consistency**. HLS can only split a segment cleanly at an **I-frame (Keyframe)**.

* **GOP Size:** Must be fixed. Use a 2-second GOP.
* For 30fps video = GOP of 60 frames.
* For 60fps video = GOP of 120 frames.


* **HLS Segment Length:** Must be a strict multiple of your GOP size. **4 seconds** (2 GOPs) or **6 seconds** (3 GOPs) is standard.
* **Scene Cut Detection:** **Disable it** (`scenecut=0` or `no-scenecut`). If enabled, the encoder inserts random I-frames at scene changes, breaking the fixed alignment and causing variable-length HLS segments, which triggers playback stutter.

---

## FFmpeg Configuration Example

If you are using FFmpeg to generate the MPEG-TS streams over UDP/RTP to your HLS segmenter, use a syntax structured like this:

```bash
ffmpeg -i input_signal_1 \
  -c:v libx264 -b:v 15M -maxrate:v 15M -bufsize:v 15M \
  -g 60 -keyint_min 60 -scenecut 0 \
  -c:a aac -b:a 256k \
  -f mpegts -muxrate 18M udp://<segmenter_ip>:<port>?pkt_size=1316

```

### Key Parameters Explained:

* **`-muxrate 18M`**: Forces the entire MPEG-TS container to a strict 18 Mbps CBR by inserting null packets. This ensures perfect predictability over your 100Mbps link. Two streams will occupy exactly 36 Mbps, leaving over 60% of the network headroom free.
* **`pkt_size=1316`**: Aligns exactly 7 MPEG-TS packets (7 × 188 bytes) into a single standard 1500-byte Ethernet MTU frame, preventing packet fragmentation and reducing CPU overhead on the segmenter.


Q: 
i have 6 channel ac3 audio now on 640 br / 1500 buffer 

A: 
With 6-channel AC-3 audio at 640 kbps, you are already using the maximum standard bitrate for AC-3 (Dolby Digital). It provides excellent audio quality, but because multichannel audio carries significantly more data payload than standard stereo tracks, it requires strict enforcement of the container settings to prevent the HLS segmenter from choking.

Here is how to align that audio profile with your video stream and buffer strategy:

## 1. Adjusting the Muxrate for AC-3 Overhead

AC-3 audio packets are heavier and have strict timing constraints. When you combine 640 kbps audio with a high-bitrate video stream (e.g., 15 Mbps), the overhead of the MPEG-TS container increases.

* If your video target is **15 Mbps**, increase your `-muxrate` from 18M to **19M (19 Mbps)**.
* This leaves a comfortable headroom buffer for the 640 kbps audio stream and prevents packet collision within the multiplexer.
* Total bandwidth for both signals will be exactly **38 Mbps**, which is still safely under your 100Mbps Ethernet ceiling.

## 2. Setting the Audio Buffer Size

The `1500` buffer value you mentioned is likely referring to a 1500-millisecond (1.5 second) audio buffer or a strict VBV setting. For multi-channel AC-3 to sync perfectly with video frames during HLS slicing:

* **Keep audio and video buffer durations identical.** If your video VBV buffer is set to 1 second (`-bufsize:v 15M`), your audio buffer should look for a matching window.
* **Audio VBV setting:** If you are defining the audio buffer explicitly in your encoder, use `-bufsize:a 640k` (a 1-second buffer) or `-bufsize:a 960k` (a 1.5-second buffer to match your current setup).

---

## 3. The Critical Step: Interleaving and PTS Alignment

HLS segmenters split files strictly on video keyframes. If the audio packets are not tightly multiplexed alongside the corresponding video packets, the segmenter will split the file while some audio data for that frame is still waiting in the buffer. This causes **audio drift** or a **noticeable pop/silence at the HLS segment boundaries**.

To fix this, force strict interleaving and match the AC-3 frame size constraints:

* **AC-3 Frame Size:** AC-3 audio always operates on fixed blocks of 1536 samples per frame (which equals exactly 32ms of audio at 48kHz).
* **Max Mux Delay:** Tell the multiplexer to lower its internal buffer delay to force frequent, small audio injections alongside the video.

## Updated FFmpeg Configuration Example

Here is how to update your streaming command to cleanly inject the 6-channel AC-3 audio into the MPEG-TS stream without disturbing the HLS segmenter:

```bash
ffmpeg -i input_signal_1 \
  -c:v libx264 -b:v 15M -maxrate:v 15M -bufsize:v 15M \
  -g 60 -keyint_min 60 -scenecut 0 \
  -c:a ac3 -b:a 640k -bufsize:a 640k \
  -max_delay 500000 \
  -f mpegts -muxrate 19M udp://<segmenter_ip>:<port>?pkt_size=1316

```

### Why this prevents HLS Segmenter errors:

* **`-max_delay 500000`:** Sets the maximum multiplexing pass-through delay to 500 milliseconds (0.5 seconds). This forces the encoder to interleave the audio and video packets closely together in time, rather than letting the 640 kbps audio pool up.
* **`-muxrate 19M`:** The strict CBR padding guarantees that the network packet arrival time at the HLS segmenter remains predictable, meaning the segmenter never has to burst-read data to catch up with the heavy AC-3 audio tracks.