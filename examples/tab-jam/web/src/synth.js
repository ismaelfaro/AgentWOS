// Tab Jam synthesizer. Works on any BaseAudioContext, so the same code plays
// live and renders offline for level checks.

const hz = (midi) => 440 * 2 ** ((midi - 69) / 12);

/** Generated impulse response: decaying stereo noise, darker towards the tail. */
function impulse(ctx, seconds) {
  const length = Math.floor(ctx.sampleRate * seconds);
  const buffer = ctx.createBuffer(2, length, ctx.sampleRate);
  for (let ch = 0; ch < 2; ch++) {
    const data = buffer.getChannelData(ch);
    let last = 0;
    for (let i = 0; i < length; i++) {
      const t = i / length;
      const white = Math.random() * 2 - 1;
      last += (white - last) * (1 - 0.75 * t); // one-pole lowpass that closes over time
      data[i] = last * (1 - t) ** 3;
    }
  }
  return buffer;
}

/** Soft clipper so peaks round off instead of crackling. */
function softClip(ctx, drive) {
  const shaper = ctx.createWaveShaper();
  const curve = new Float32Array(1024);
  for (let i = 0; i < curve.length; i++) {
    const x = (i / (curve.length - 1)) * 2 - 1;
    curve[i] = Math.tanh(x * drive) / Math.tanh(drive);
  }
  shaper.curve = curve;
  shaper.oversample = "2x";
  return shaper;
}

export function createSynth(ctx) {
  // master: bus -> compressor -> soft clip -> volume -> out
  const master = ctx.createGain();
  const compressor = ctx.createDynamicsCompressor();
  compressor.threshold.value = -16;
  compressor.ratio.value = 3;
  compressor.attack.value = 0.005;
  compressor.release.value = 0.18;
  const volume = ctx.createGain();
  volume.gain.value = 0.8;
  master.connect(compressor).connect(softClip(ctx, 1.4)).connect(volume).connect(ctx.destination);

  // shared reverb and a tempo-synced delay for the lead
  const reverb = ctx.createConvolver();
  reverb.buffer = impulse(ctx, 1.8);
  const reverbReturn = ctx.createGain();
  reverbReturn.gain.value = 0.5;
  reverb.connect(reverbReturn).connect(master);

  const delay = ctx.createDelay(1.5);
  const feedback = ctx.createGain();
  feedback.gain.value = 0.32;
  const delayTone = ctx.createBiquadFilter();
  delayTone.type = "lowpass";
  delayTone.frequency.value = 2200;
  const delayReturn = ctx.createGain();
  delayReturn.gain.value = 0.28;
  delay.connect(delayTone).connect(feedback).connect(delay);
  delayTone.connect(delayReturn).connect(master);

  // the bass dips slightly under each kick so the low end stays clear
  const bassBus = ctx.createGain();
  bassBus.connect(master);

  const noise = ctx.createBuffer(1, ctx.sampleRate, ctx.sampleRate);
  const noiseData = noise.getChannelData(0);
  for (let i = 0; i < noiseData.length; i++) noiseData[i] = Math.random() * 2 - 1;

  function voice(t, peak, attack, decay, { pan = 0, out = master, reverbSend = 0, delaySend = 0 } = {}) {
    const gain = ctx.createGain();
    gain.gain.setValueAtTime(0.0001, t);
    gain.gain.exponentialRampToValueAtTime(Math.max(peak, 0.0002), t + attack);
    gain.gain.exponentialRampToValueAtTime(0.0001, t + attack + decay);
    const panner = ctx.createStereoPanner();
    panner.pan.value = pan;
    gain.connect(panner).connect(out);
    for (const [send, target] of [[reverbSend, reverb], [delaySend, delay]]) {
      if (send > 0) {
        const s = ctx.createGain();
        s.gain.value = send;
        panner.connect(s).connect(target);
      }
    }
    return gain;
  }

  function osc(type, freq, t, stop, detune = 0) {
    const o = ctx.createOscillator();
    o.type = type;
    o.frequency.setValueAtTime(freq, t);
    o.detune.value = detune;
    o.start(t);
    o.stop(stop);
    return o;
  }

  function noiseBurst(t, stop, type, freq, q = 1) {
    const src = ctx.createBufferSource();
    src.buffer = noise;
    src.loop = true;
    src.loopStart = Math.random() * 0.5;
    const filter = ctx.createBiquadFilter();
    filter.type = type;
    filter.frequency.value = freq;
    filter.Q.value = q;
    src.connect(filter);
    src.start(t, src.loopStart);
    src.stop(stop);
    return filter;
  }

  function kick(t, v) {
    const end = t + 0.5;
    const body = osc("sine", 165, t, end);
    body.frequency.exponentialRampToValueAtTime(48, t + 0.09);
    body.frequency.exponentialRampToValueAtTime(40, t + 0.4);
    body.connect(voice(t, v * 0.95, 0.002, 0.34));
    // short click so the kick reads on small speakers
    noiseBurst(t, t + 0.03, "highpass", 2500).connect(voice(t, v * 0.18, 0.001, 0.015));
    bassBus.gain.cancelScheduledValues(t);
    bassBus.gain.setValueAtTime(0.75, t);
    bassBus.gain.linearRampToValueAtTime(1, t + 0.1);
  }

  function snare(t, v) {
    const end = t + 0.3;
    noiseBurst(t, end, "bandpass", 3200, 0.7).connect(voice(t, v * 0.5, 0.001, 0.16, { reverbSend: 0.25 }));
    const tone = osc("triangle", 215, t, end);
    tone.frequency.exponentialRampToValueAtTime(170, t + 0.07);
    tone.connect(voice(t, v * 0.35, 0.001, 0.09));
  }

  function hat(t, v) {
    const open = v > 0.5;
    noiseBurst(t, t + 0.25, "highpass", 8200)
      .connect(voice(t, v * 0.22, 0.001, open ? 0.09 : 0.035, { pan: 0.3 }));
  }

  /** FM pair: a sine carrier whose pitch is modulated by another sine. */
  function fm(freq, ratio, index, indexDecay, t, stop) {
    const carrier = osc("sine", freq, t, stop);
    const modulator = osc("sine", freq * ratio, t, stop);
    const amount = ctx.createGain();
    amount.gain.setValueAtTime(freq * index, t);
    amount.gain.exponentialRampToValueAtTime(Math.max(freq * index * 0.05, 0.01), t + indexDecay);
    modulator.connect(amount).connect(carrier.frequency);
    return carrier;
  }

  // Plucked string bass: a round fundamental, a little string growl that fades
  // fast, and a finger thump at the start. Notes ring past their length.
  function bass(t, v, midi, len) {
    const ring = Math.min(1.6, Math.max(0.35, len * 1.6));
    const end = t + ring + 0.1;
    const freq = hz(midi);
    const tone = ctx.createBiquadFilter();
    tone.type = "lowpass";
    tone.Q.value = 0.8;
    tone.frequency.setValueAtTime(1400 + 1200 * v, t);
    tone.frequency.exponentialRampToValueAtTime(320, t + 0.18);
    osc("triangle", freq, t, end).connect(tone);
    fm(freq, 2, 1.2 * v, 0.12, t, end).connect(tone);
    tone.connect(voice(t, v * 0.55, 0.004, ring, { out: bassBus, reverbSend: 0.05 }));
    noiseBurst(t, t + 0.04, "bandpass", 700, 1.2).connect(voice(t, v * 0.1, 0.001, 0.025, { out: bassBus }));
  }

  // ---- lead voices -------------------------------------------------------
  // Every lead voice ends in this: level, pan and effect sends for one note.
  function leadOut(t, peak, attack, hold, release, { reverbSend = 0.2, delaySend = 0, sustain = true } = {}) {
    const gain = ctx.createGain();
    gain.gain.setValueAtTime(0.0001, t);
    gain.gain.exponentialRampToValueAtTime(peak, t + attack);
    if (sustain) gain.gain.setValueAtTime(peak, t + hold);
    gain.gain.exponentialRampToValueAtTime(0.0001, t + hold + release);
    const panner = ctx.createStereoPanner();
    panner.pan.value = -0.15;
    gain.connect(panner).connect(master);
    for (const [amount, target] of [[reverbSend, reverb], [delaySend, delay]]) {
      if (amount > 0) {
        const send = ctx.createGain();
        send.gain.value = amount;
        panner.connect(send).connect(target);
      }
    }
    return gain;
  }

  const leadVoices = {
    // Single saw with a reinforced fundamental and a fixed gentle low-pass,
    // held at one level. Matched to a reference recording.
    saw(t, v, freq, dur) {
      const hold = Math.max(0.08, dur * 0.98), end = t + hold + 0.11;
      const tone = ctx.createBiquadFilter();
      tone.type = "lowpass";
      tone.frequency.value = 3400;
      tone.Q.value = 0.6;
      osc("sawtooth", freq, t, end).connect(tone);
      const fundamental = ctx.createGain();
      fundamental.gain.value = 0.17;
      osc("sine", freq, t, end).connect(fundamental).connect(tone);
      tone.connect(leadOut(t, v * 0.2, 0.004, hold, 0.09, { reverbSend: 0.18 }));
    },

    // Three detuned saws over a square sub, a resonant filter that opens on the
    // attack, a glide from the previous note and vibrato on held notes.
    supersaw(t, v, freq, dur) {
      const hold = Math.max(0.1, dur * 0.95), end = t + hold + 0.3;
      const from = lastLeadFreq ?? freq;
      const filter = ctx.createBiquadFilter();
      filter.type = "lowpass";
      filter.Q.value = 7;
      filter.frequency.setValueAtTime(700, t);
      filter.frequency.exponentialRampToValueAtTime(1800 + 3200 * v, t + 0.03);
      filter.frequency.exponentialRampToValueAtTime(900, t + Math.max(hold, 0.12));
      const vibrato = osc("sine", 5.8, t, end);
      const depth = ctx.createGain();
      depth.gain.setValueAtTime(0, t + Math.min(hold, 0.18));
      depth.gain.linearRampToValueAtTime(hold > 0.3 ? 9 : 0, t + Math.min(hold, 0.45));
      vibrato.connect(depth);
      const mix = ctx.createGain();
      mix.gain.value = 0.3;
      mix.connect(filter);
      for (const [type, detune, level] of [["sawtooth", -12, 1], ["sawtooth", 0, 1], ["sawtooth", 12, 1], ["square", -1200, 0.5]]) {
        const o = osc(type, from, t, end, detune);
        o.frequency.exponentialRampToValueAtTime(freq, t + 0.035);
        depth.connect(o.detune);
        const g = ctx.createGain();
        g.gain.value = level;
        o.connect(g).connect(mix);
      }
      filter.connect(leadOut(t, v * 0.52, 0.006, hold, 0.2, { reverbSend: 0.3, delaySend: 0.3, sustain: false }));
    },

    // Electric piano: FM tine with a bell at the attack that mellows as it decays.
    piano(t, v, freq, dur) {
      const ring = Math.min(2.4, Math.max(0.5, dur * 2.2)), end = t + ring + 0.3;
      const body = ctx.createBiquadFilter();
      body.type = "lowpass";
      body.frequency.value = 4200;
      fm(freq, 1, 0.9 + 1.4 * v, ring * 0.6, t, end).connect(body);
      const bell = ctx.createGain();
      bell.gain.setValueAtTime(0.25 * v, t);
      bell.gain.exponentialRampToValueAtTime(0.001, t + 0.12);
      fm(freq, 14, 0.5, 0.06, t, t + 0.2).connect(bell).connect(body);
      body.connect(leadOut(t, v * 0.3, 0.003, 0, ring, { reverbSend: 0.3, delaySend: 0.06, sustain: false }));
    },

    // Hollow square wave with a quick blip of pitch at the start, like an
    // old game console. Dry and exact.
    square(t, v, freq, dur) {
      const hold = Math.max(0.06, dur * 0.85), end = t + hold + 0.06;
      const o = osc("square", freq * 2, t, end);
      o.frequency.exponentialRampToValueAtTime(freq, t + 0.012);
      const vibrato = osc("square", 7, t, end);
      const depth = ctx.createGain();
      depth.gain.setValueAtTime(0, t + Math.min(hold, 0.25));
      depth.gain.setValueAtTime(hold > 0.35 ? 12 : 0, t + Math.min(hold, 0.26));
      vibrato.connect(depth).connect(o.detune);
      const tone = ctx.createBiquadFilter();
      tone.type = "lowpass";
      tone.frequency.value = 5200;
      o.connect(tone).connect(leadOut(t, v * 0.17, 0.002, hold, 0.04, { reverbSend: 0.05, delaySend: 0.18 }));
    },

    // Bell: FM with a non-harmonic ratio, so the overtones clang, and a long ring.
    bell(t, v, freq, dur) {
      const ring = Math.min(3, Math.max(0.9, dur * 3)), end = t + ring + 0.3;
      const out = leadOut(t, v * 0.26, 0.002, 0, ring, { reverbSend: 0.5, delaySend: 0.12, sustain: false });
      fm(freq, 3.5, 2.2 * v + 0.8, ring * 0.35, t, end).connect(out);
      const shimmer = ctx.createGain();
      shimmer.gain.setValueAtTime(0.3, t);
      shimmer.gain.exponentialRampToValueAtTime(0.001, t + ring * 0.25);
      fm(freq * 2, 5.4, 1.5, 0.2, t, end).connect(shimmer).connect(out);
    },
  };
  let lastLeadFreq = null;
  let leadSound = "saw";

  // Optional sampled lead: one looped recording, re-pitched per note.
  let sample = null;
  function sampledLead(t, v, midi, len) {
    const hold = Math.max(0.1, len * 0.98);
    const release = 0.12;
    const src = ctx.createBufferSource();
    src.buffer = sample.buffer;
    src.loop = true;
    src.playbackRate.value = 2 ** ((midi - sample.midi) / 12);
    const gain = ctx.createGain();
    const peak = v * 0.55;
    gain.gain.setValueAtTime(0.0001, t);
    gain.gain.exponentialRampToValueAtTime(peak, t + 0.012);
    gain.gain.setValueAtTime(peak, t + hold);
    gain.gain.exponentialRampToValueAtTime(0.0001, t + hold + release);
    const panner = ctx.createStereoPanner();
    panner.pan.value = -0.15;
    src.connect(gain).connect(panner).connect(master);
    const send = ctx.createGain();
    send.gain.value = 0.2;
    panner.connect(send).connect(reverb);
    // start at a different point of the loop each time so repeats do not sound stamped
    src.start(t, Math.random() * sample.buffer.duration);
    src.stop(t + hold + release + 0.02);
  }

  function lead(t, v, midi, len) {
    if (leadSound === "sample" && sample) return sampledLead(t, v, midi, len);
    const freq = hz(midi);
    (leadVoices[leadSound] ?? leadVoices.saw)(t, v, freq, len);
    lastLeadFreq = freq;
  }

  // ---- orchestra sections ---------------------------------------------------
  // Each timbre wires its oscillators into `out`. `spread` is the section's
  // detune in cents: its own voices sit that far apart, which is what makes
  // one section sound different from the next on the same note.
  const sectionTimbres = {
    strings(freq, spread, t, end, out) {
      const tone = ctx.createBiquadFilter();
      tone.type = "lowpass";
      tone.frequency.value = 2400;
      for (const d of [-spread, spread / 2, spread]) osc("sawtooth", freq, t, end, d).connect(tone);
      tone.connect(out);
    },
    brass(freq, spread, t, end, out) {
      const tone = ctx.createBiquadFilter();
      tone.type = "lowpass";
      tone.Q.value = 2;
      tone.frequency.setValueAtTime(700, t);
      tone.frequency.exponentialRampToValueAtTime(2600, t + 0.08);
      tone.frequency.exponentialRampToValueAtTime(1300, t + 0.5);
      for (const d of [-spread / 2, spread / 2]) osc("sawtooth", freq, t, end, d).connect(tone);
      const level = ctx.createGain();
      level.gain.value = 1.3;
      tone.connect(level).connect(out);
    },
    flute(freq, spread, t, end, out) {
      const vibrato = osc("sine", 5, t, end);
      const depth = ctx.createGain();
      depth.gain.value = 6;
      vibrato.connect(depth);
      const body = ctx.createGain();
      body.gain.value = 1.1;
      for (const [type, level, d] of [["sine", 1, 0], ["triangle", 0.35, spread]]) {
        const o = osc(type, freq * 2, t, end, d);
        depth.connect(o.detune);
        const g = ctx.createGain();
        g.gain.value = level;
        o.connect(g).connect(body);
      }
      const breath = ctx.createGain();
      breath.gain.value = 0.12;
      noiseBurst(t, end, "bandpass", freq * 4, 3).connect(breath).connect(body);
      body.connect(out);
    },
    choir(freq, spread, t, end, out) {
      const voices = ctx.createGain();
      for (const d of [-spread, 0, spread]) osc("sawtooth", freq, t, end, d).connect(voices);
      // Three band-pass peaks shape the saws into an "aah".
      for (const [formant, q, level] of [[600, 5, 3.8], [1040, 6, 2.6], [2250, 7, 1.4]]) {
        const f = ctx.createBiquadFilter();
        f.type = "bandpass";
        f.frequency.value = formant;
        f.Q.value = q;
        const g = ctx.createGain();
        g.gain.value = level;
        voices.connect(f).connect(g).connect(out);
      }
    },
    organ(freq, spread, t, end, out) {
      [[1, 1], [2, 0.6], [3, 0.35], [4, 0.25], [6, 0.12]].forEach(([harmonic, level], i) => {
        const g = ctx.createGain();
        g.gain.value = level * 0.9;
        osc("sine", freq * harmonic, t, end, i % 2 ? spread : -spread).connect(g).connect(out);
      });
    },
  };

  // One held note of a section for a bar. Its level pulses on a division of
  // the bar, so every section breathes on the band's clock whatever its rate.
  function section(t, n, step) {
    const dur = n.len * step;
    const end = t + dur + 0.3;
    // Sharing the level across sounding sections keeps a full orchestra from clipping.
    const level = (n.vel / 127) * 0.2 / Math.sqrt(n.count);
    const pulse = n.pulse * step;

    const gate = ctx.createGain();
    gate.gain.setValueAtTime(0.0001, t);
    gate.gain.linearRampToValueAtTime(level * 0.3, t + 0.02);
    for (let p = t; p < t + dur - 0.001; p += pulse) {
      gate.gain.linearRampToValueAtTime(level, p + Math.min(0.25, pulse * 0.3));
      gate.gain.linearRampToValueAtTime(level * 0.3, p + pulse * 0.96);
    }
    gate.gain.linearRampToValueAtTime(0.0001, t + dur + 0.25);

    (sectionTimbres[n.timbre] ?? sectionTimbres.strings)(hz(n.midi), n.detune, t, end, gate);
    const panner = ctx.createStereoPanner();
    panner.pan.value = ((n.voice * 0.618) % 1) * 1.6 - 0.8;
    gate.connect(panner).connect(master);
    const send = ctx.createGain();
    send.gain.value = 0.35;
    panner.connect(send).connect(reverb);
  }

  return {
    leadSounds: Object.keys(leadVoices),
    /** One of `leadSounds`, or "sample" once a recording is loaded. */
    setLeadSound(name) {
      leadSound = name;
    },
    /**
     * Makes a recording available as the "sample" lead sound.
     * `midi` is the pitch the recording sounds at.
     */
    setLeadSample(buffer, midi) {
      sample = buffer ? { buffer, midi } : null;
    },
    /** `step` is the length of a sixteenth note in seconds. */
    playNote(n, t, step) {
      const v = n.vel / 127;
      if (n.inst === "kick") kick(t, v);
      else if (n.inst === "snare") snare(t, v);
      else if (n.inst === "hat") hat(t, v);
      else if (n.inst === "bass") bass(t, v, n.midi, n.len * step);
      else if (n.inst === "lead") lead(t, v, n.midi, n.len * step);
      else if (n.inst === "drone") section(t, n, step);
    },
    /** Dotted-eighth echo that follows the tempo. */
    setTempo(bpm) {
      delay.delayTime.setTargetAtTime((60 / bpm) * 0.75, ctx.currentTime, 0.05);
    },
    setVolume(value) {
      volume.gain.setTargetAtTime(value, ctx.currentTime, 0.02);
    },
  };
}
