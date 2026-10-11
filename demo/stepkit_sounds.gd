## StepKitSounds: Procedural default sound effects for the SM64 movement plugin.
##
## Generates simple synthesized SFX at runtime (no audio files needed).
## Connect to StepChar3D's `sound_played` signal to trigger playback.
##
## Usage:
##   var sounds = StepKitSounds.new()
##   add_child(sounds)
##   step_char.sound_played.connect(sounds.play_sound)

extends Node
class_name StepKitSounds

## Volume for all sounds (0.0 to 1.0).
@export var volume: float = 0.5

var _players: Array[AudioStreamPlayer] = []
var _streams: Dictionary = {}

func _ready() -> void:
    # Generate procedural sounds for each event.
    _streams["Jump"] = _make_tone(440.0, 880.0, 0.15)  # Rising boing
    _streams["Double Jump"] = _make_tone(520.0, 1040.0, 0.15)
    _streams["Triple Jump"] = _make_tone(600.0, 1200.0, 0.18)
    _streams["Flip"] = _make_tone(500.0, 1000.0, 0.2)
    _streams["Dive"] = _make_noise(0.25)  # Whoosh
    _streams["Punch"] = _make_noise(0.08)  # Thwip
    _streams["Kick"] = _make_noise(0.1)
    _streams["Ground Pound"] = _make_thud(0.3)
    _streams["Bonk"] = _make_tone(200.0, 100.0, 0.15)  # Descending bonk
    _streams["Landing"] = _make_thud(0.12)
    _streams["Step"] = _make_thud(0.05)
    _streams["Fall Scream"] = _make_tone(800.0, 400.0, 0.5)  # WAAAOOOW
    _streams["Swim"] = _make_noise(0.2)
    _streams["Slide"] = _make_noise(0.3)

    # Pre-create players for polyphony.
    for i in range(8):
        var p = AudioStreamPlayer.new()
        p.volume_db = linear_to_db(volume)
        add_child(p)
        _players.append(p)

## Play a sound by event name (from StepChar3D.sound_played signal).
func play_sound(event_name: String) -> void:
    if not _streams.has(event_name):
        return
    # Find a free player.
    for p in _players:
        if not p.playing:
            p.stream = _streams[event_name]
            p.play()
            return
    # All busy — steal the first one.
    _players[0].stream = _streams[event_name]
    _players[0].play()

## Generate a tone that sweeps from freq_start to freq_end.
func _make_tone(freq_start: float, freq_end: float, duration: float) -> AudioStreamWAV:
    var sample_rate := 22050
    var frames := int(sample_rate * duration)
    var data := PackedByteArray()
    data.resize(frames * 2)  # 16-bit mono
    for i in range(frames):
        var t := float(i) / sample_rate
        var freq := lerpf(freq_start, freq_end, float(i) / frames)
        var sample := sin(TAU * freq * t) * (1.0 - float(i) / frames)  # Decay
        var val := int(sample * 32767.0 * 0.5)
        data.encode_s16(i * 2, val)
    var stream := AudioStreamWAV.new()
    stream.format = AudioStreamWAV.FORMAT_16_BITS
    stream.mix_rate = sample_rate
    stream.data = data
    return stream

## Generate white noise burst.
func _make_noise(duration: float) -> AudioStreamWAV:
    var sample_rate := 22050
    var frames := int(sample_rate * duration)
    var data := PackedByteArray()
    data.resize(frames * 2)
    for i in range(frames):
        var sample := randf_range(-1.0, 1.0) * (1.0 - float(i) / frames)
        var val := int(sample * 32767.0 * 0.3)
        data.encode_s16(i * 2, val)
    var stream := AudioStreamWAV.new()
    stream.format = AudioStreamWAV.FORMAT_16_BITS
    stream.mix_rate = sample_rate
    stream.data = data
    return stream

## Generate a low thud.
func _make_thud(duration: float) -> AudioStreamWAV:
    var sample_rate := 22050
    var frames := int(sample_rate * duration)
    var data := PackedByteArray()
    data.resize(frames * 2)
    for i in range(frames):
        var t := float(i) / sample_rate
        var freq := 120.0 * (1.0 - float(i) / frames * 0.5)
        var sample := sin(TAU * freq * t) * (1.0 - float(i) / frames)
        var val := int(sample * 32767.0 * 0.6)
        data.encode_s16(i * 2, val)
    var stream := AudioStreamWAV.new()
    stream.format = AudioStreamWAV.FORMAT_16_BITS
    stream.mix_rate = sample_rate
    stream.data = data
    return stream
