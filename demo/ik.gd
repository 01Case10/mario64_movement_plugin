class_name StepKitIK
extends RefCounted
## Analytic two-bone IK solver for the StepBot reference rig.
##
## The sim publishes hand-target hints (world-space); this solver places the
## two-bone arms (shoulder -> elbow -> hand) to reach them. The sim never
## solves IK itself — it stays rig-agnostic. A designer replacing the StepBot
## never uses this; it exists only for the reference demo.

## Solve a two-bone arm.
##
## shoulder_pos: shoulder pivot world position.
## target_pos: desired hand world position (from sim IK hints).
## l1, l2: upper-arm and forearm lengths.
## pole_dir: world-space hint for elbow bend direction (elbows point here).
## shoulder_parent_basis: global basis of the shoulder pivot's parent, used
##   to return the shoulder rotation in parent-local space.
##
## Returns [shoulder_basis_local, elbow_basis_local] as Basis values.
## Beyond max reach the arm fully extends toward the target (graceful).
## Twist around the bone axes is arbitrary: both segments are capsules
## (radially symmetric), so only the -Y aim direction matters visually.
static func solve_arm(
	shoulder_pos: Vector3,
	target_pos: Vector3,
	l1: float,
	l2: float,
	pole_dir: Vector3,
	shoulder_parent_basis: Basis,
) -> Array:
	var to_target := target_pos - shoulder_pos
	var dist := to_target.length()
	var max_reach := l1 + l2 - 0.001
	var min_reach := absf(l1 - l2) + 0.001
	var d := clampf(dist, min_reach, max_reach)
	var dir := to_target / maxf(dist, 0.0001)

	# Elbow position via law of cosines: angle at the shoulder between
	# the target direction and the upper arm.
	var cos_a := clampf((l1 * l1 + d * d - l2 * l2) / (2.0 * l1 * d), -1.0, 1.0)
	var sin_a := sqrt(maxf(0.0, 1.0 - cos_a * cos_a))
	# Pole component perpendicular to the aim direction.
	var pole_perp := pole_dir - dir * pole_dir.dot(dir)
	if pole_perp.length() < 0.001:
		pole_perp = Vector3.UP - dir * Vector3.UP.dot(dir)
		if pole_perp.length() < 0.001:
			pole_perp = Vector3.RIGHT - dir * Vector3.RIGHT.dot(dir)
	pole_perp = pole_perp.normalized()
	var elbow_pos := shoulder_pos + dir * (l1 * cos_a) + pole_perp * (l1 * sin_a)

	# Shoulder: aim local -Y (the arm's rest direction) at the elbow.
	var upper_dir := (elbow_pos - shoulder_pos).normalized()
	var shoulder_global := _basis_neg_y_toward(upper_dir, pole_dir)
	var shoulder_local: Basis = shoulder_parent_basis.inverse() * shoulder_global

	# Elbow: aim local -Y at the clamped target, expressed in shoulder space
	# (the elbow pivot is a child of the shoulder pivot).
	var clamped_target := shoulder_pos + dir * d
	var fore_dir_world := (clamped_target - elbow_pos).normalized()
	var fore_dir_local: Vector3 = shoulder_global.inverse() * fore_dir_world
	var elbow_local := _basis_neg_y_toward(fore_dir_local, Vector3.UP)

	return [shoulder_local, elbow_local]


## Build a Basis whose -Y axis points along `direction`.
## `pole` biases the twist (rotation about the aim axis); irrelevant for
## radially-symmetric capsule meshes, but keeps the basis well-defined.
static func _basis_neg_y_toward(direction: Vector3, pole: Vector3) -> Basis:
	var y := -direction.normalized()
	var x := y.cross(pole)
	if x.length() < 0.0001:
		x = y.cross(Vector3.UP)
		if x.length() < 0.0001:
			x = y.cross(Vector3.RIGHT)
	x = x.normalized()
	var z := x.cross(y).normalized()
	x = y.cross(z).normalized()
	return Basis(x, y, z)
