extends Node3D
## Automated mesh-vs-world clip check for the moveset showcase.
##
## The sim harness cannot see poses: the sim capsule is correct in every case
## here, only the posed StepBot meshes clip. So this check runs in Godot once
## per rendered frame (after the StepBot pose is applied) and tests each
## StepBot mesh's oriented bounding box against the static level geometry via
## the physics space.
##
## Each mesh AABB is INSET by 0.08 m on all sides: touching no longer counts,
## only real penetration. There are NO action exemptions: the hang and climb
## are designed (sim-side) to never intersect, so any hit is a real bug.

const INSET := 0.08
const MIN_SIZE := 0.02
const MESH_NAMES := ["Body", "Head", "ArmL", "ArmR", "LegL", "LegR"]
# Write the report a few frames before the showcase quits.
const REPORT_TICK := 1565

var _meshes: Array[MeshInstance3D] = []
var _hits: Array = []  # each: [frame, action_name, mesh_name]
var _frame := 0
var _shape := BoxShape3D.new()
var _params := PhysicsShapeQueryParameters3D.new()
var _reported := false


func _ready() -> void:
	var stepbot: Node3D = get_parent().get_node("Player/StepBot")
	for n in MESH_NAMES:
		_meshes.append(stepbot.get_node(n) as MeshInstance3D)
	_params.collide_with_areas = false
	_params.collide_with_bodies = true
	_params.collision_mask = 1
	_params.shape = _shape


func _process(_delta: float) -> void:
	_frame += 1
	var showcase = get_parent()
	if int(showcase.tick) >= REPORT_TICK and not _reported:
		_write_report()
		return
	var stepbot: Node3D = showcase.get_node("Player/StepBot")
	var action: int = stepbot.action
	var space := get_world_3d().direct_space_state
	var aname := ""
	for mi in _meshes:
		var aabb: AABB = mi.mesh.get_aabb()
		var c := aabb.get_center()
		var sz := aabb.size - Vector3(INSET * 2.0, INSET * 2.0, INSET * 2.0)
		sz.x = maxf(sz.x, MIN_SIZE)
		sz.y = maxf(sz.y, MIN_SIZE)
		sz.z = maxf(sz.z, MIN_SIZE)
		_shape.size = sz
		# Oriented box: mesh global rotation, centered on the mesh AABB.
		_params.transform = mi.global_transform * Transform3D(Basis.IDENTITY, c)
		var hits := space.intersect_shape(_params, 4)
		if not hits.is_empty():
			if aname == "":
				aname = str(showcase.ACTION_NAMES.get(action, "0x%08X" % action))
			_hits.append([_frame, aname, mi.name])
			print("CLIP_CHECK frame %d action %s mesh %s" % [_frame, aname, mi.name])


func _write_report() -> void:
	_reported = true
	var status := "PASS" if _hits.is_empty() else "FAIL"
	var lines: Array[String] = []
	lines.append("CLIP CHECK v6: " + status)
	lines.append("frames checked: %d" % _frame)
	lines.append("hits: %d" % _hits.size())
	for h in _hits:
		lines.append("frame %d action %s mesh %s" % [h[0], h[1], h[2]])
	var path := OS.get_environment("HOME") + "/workspace/your_files/clip_check_v6.txt"
	var f := FileAccess.open(path, FileAccess.WRITE)
	if f:
		f.store_string("\n".join(lines) + "\n")
		f.close()
	print("==== CLIP CHECK v6: %s (%d hits over %d frames) ====" % [status, _hits.size(), _frame])
	for h in _hits:
		print("  frame %d action %s mesh %s" % [h[0], h[1], h[2]])
