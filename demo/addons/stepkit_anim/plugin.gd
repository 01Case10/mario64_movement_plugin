@tool
extends EditorPlugin
## StepKit animation validator dock: runs `stepkit anim validate` and shows results.

var dock: Control

func _enter_tree() -> void:
	dock = preload("res://addons/stepkit_anim/dock.tscn").instantiate()
	dock.get_node("VBox/ValidateButton").pressed.connect(_on_validate)
	add_control_to_dock(DOCK_SLOT_RIGHT_UL, dock)

func _exit_tree() -> void:
	remove_control_from_docks(dock)
	dock.free()

func _on_validate() -> void:
	var output: TextEdit = dock.get_node("VBox/Output")
	output.text = "Running stepkit anim validate...\n"
	var manifest: LineEdit = dock.get_node("VBox/ManifestPath")
	var clips: LineEdit = dock.get_node("VBox/ClipsPath")
	var stepkit := ProjectSettings.get_setting("stepkit/cli_path", "stepkit")
	var args := ["anim", "validate", "--manifest", manifest.text, "--clips", clips.text]
	var result := []
	var code := OS.execute(stepkit, args, result, true)
	output.text += "".join(result)
	output.text += "\nExit code: %d\n" % code
