# Circular cubic candidate: derivation and limitations

Private source review, NOT RUN. This is a derivation for independent review,
not an executed formal proof or a qualified public API.

## Provenance

The construction is the standard tangent/midpoint circular cubic. Its
mathematical context was read in A. Riskus, “Approximation of a Cubic Bezier
Curve by Circular Arcs and Vice Versa”, Information Technology and Control
35(4), 2006, section3:
https://www.itc.ktu.lt/index.php/ITC/article/view/11812
https://doi.org/10.5755/j01.itc.35.4.11812

Only mathematical context is used from that primary paper; no external code,
figures, templates or assets are imported. The bound below is derived here.
The original project reference is CamWight's P1, retained with its authorship:
https://github.com/CamWight/solidcraft/blob/7dc82dc7/crates/io/src/paper/pdf.rs
and its continuation:
https://github.com/0xlillith/solidcraft/blob/effaf49113dadafc2e16663972ed97e015cb5bf4/crates/io/src/paper/pdf.rs
Those project contributions are MIT OR Apache-2.0. The new enclosure/Taylor
implementation and tests are original SolidCraft contributor work.
No GPL-family implementation is used or copied.

## 1. Exact-real construction and a radial polynomial

Let one positive subarc have angle theta<=pi/2, unit radius and centre0.
Put alpha=theta/2, beta=theta/4, c=cos(alpha), s=sin(alpha),
k=(4/3)tan(beta). Rotate the arc symmetrically about the positive x-axis.

Its ideal cubic controls are
P0=(c,-s),
P1=(c+k*s,-s+k*c),
P2=(c+k*s,s-k*c),
P3=(c,s).

They interpolate both endpoints, have the circle's endpoint tangents, and
interpolate the arc midpoint at t=1/2. For the Bernstein cubic B(t), direct
polynomial expansion gives

  ||B(t)||^2 - 1
    = 16*sin(beta)^6/cos(beta)^2 * t^2*(1-t)^2*(2*t-1)^2.

A check on the coefficient: the cubic's leading y coefficient is
6*k*c-4*s = -8*sin(beta)^3/cos(beta). Its square equals the leading
degree6 coefficient on the right. Both endpoint double roots and the
midpoint double root determine the remaining polynomial.

Write z=t*(1-t), so 0<=z<=1/4. The remaining factor is
z^2*(1-4*z), whose derivative is 2*z*(1-6*z); hence its maximum
is1/108 at z=1/6. Therefore

  0 <= ||B(t)||^2-1 <= (4/27)*sin(beta)^6/cos(beta)^2 = Q.

The identity sqrt(1+q)-1=q/(sqrt(1+q)+1)<=q/2 gives an outward
radial error no greater than

  radius*(2/27)*sin(beta)^6/cos(beta)^2.

For this sector the ideal cubic remains inside the angular wedge, has
positive distance from the centre, and its continuous angle covers both
endpoint angles. Radial projection and the intermediate value theorem
therefore supply both directions of a Hausdorff bound to the true circular
subarc. Reflection, translation and scaling preserve the argument; a
negative sweep uses the reflected construction.

## 2. A looser bound needing no transcendental estimate

For 0<=beta<=pi/8, sin(beta)<=beta and
cos(beta)^2=1-sin(beta)^2>=1-beta^2>0. Thus

  Eideal <= radius*(2/27)*beta^6/(1-beta^2).

The candidate uses this larger quantity, not a claim that the tight maximum
is attained. Every multiplication/division/addition used to evaluate it is
outward-rounded. The interval for theta/n contains the exact-real quotient
of the represented input sweep. Using its largest absolute endpoint gives
a conservative beta for the inequality.

This deliberately refuses some tolerances that the tighter exact formula
could meet. That is an explicit limitation, not a license to coarsen output.

## 3. Enclosing the actual stored control points

An exact f64 input denotes its exact real number. Each arithmetic operation
encloses the two rounded endpoints by their next representable neighbours.
Multiplication takes all endpoint products. Division rejects intervals
containing zero, encloses reciprocal endpoints and multiplies. Nonfinite
enclosures refuse.

The scalar model is ordinary IEEE754 binary64 round-to-nearest with gradual
underflow, as required by the source's Rust floating operations. No
fast-math/flush-to-zero transformation is qualified by this derivation.
No numerical enclosure assertion was executed in this preparation.

For the trigonometric expressions the implementation calls no system
sin/cos/tan. It uses24 Maclaurin terms with outward interval recurrence:
sin through degree47 and cos through degree46. The next zero coefficients
allow Taylor/Lagrange remainder bounds

  |Rsin| <= |x|^49/49!,
  |Rcos| <= |x|^48/48!.

The derivatives have absolute value at most1 globally. An outward recurrence
computes those tails from the interval's largest absolute x. This covers
both arithmetic roundoff and the truncation remainder, without assuming a
platform libm accuracy. Tangent is the enclosed sin/cos quotient of the
small half-step; division refuses if its denominator enclosure crosses0.

Each exact ideal control has an enclosing x and y interval. The stored
control is a representable midpoint clipped into those intervals; its
maximum x displacement plus maximum y displacement bounds its Euclidean
displacement. The nonnegative Bernstein weights sum to1, so

  Econtrols = max over four controls of (x_displacement+y_displacement)

bounds the entire stored cubic's perturbation from the ideal cubic.
The outward sum Eideal+Econtrols therefore bounds the emitted piece against
the exact-real circular subarc. The greatest such bound covers the result.

Uniform subarc endpoints use the same represented inputs and the same
expression for the same j/n; adjacent pieces therefore store identical
endpoints. Different n values can choose slightly different endpoint
midpoints, still covered by their bounds. Collapsed endpoints or endpoint
handles are refused rather than emitted as a degenerate success.

## 4. Caps, failures and unconnected consumers

The existing Seg2::check admission is retained. This candidate further
admits only start angles in[-PI,PI], nonzero signed sweeps at most a quarter
turn, positive finite mm allowances, and caller caps in1..=64.
It tries at most64 subdivisions; trial storage is at most64 segments,
using try_reserve_exact. If a trial cannot meet the complete allowance,
it is discarded. No partial Vec is returned as success. Allocation and
arithmetic refusal propagate, with no input mutation.

The64-trial search has at most2080 trial pieces; the24-term trigonometric
recurrences and49/48 remainder recurrences have fixed bounds per piece.
A later scene consumer still owns aggregate work/output budgets over many
arcs; this single-arc helper does not qualify them.

An admitted Arc checks its stored centre and radius, not the future cubic's
control-point box. Returned controls are finite writer geometry, not a claim
that reinserting them as authored Scene cubics passes the original Scene
coordinate cap. No Scene cap is changed; a future adapter must respect its
actual destination's admission rules.

Later grid snapping, replacing endpoints with walked endpoints, unit
conversion, SVG ellipse support and PDF writing are NOT included. A caller
must reserve their own additional error, preserve each walked endpoint,
and prove its adapter. In particular the existing PDF subdivision works in
grid STEPS while this candidate uses MILLIMETRES; raw transplantation is
incorrect. No existing grid rule, PDF tolerance, writer, record or schema
was changed.

## 5. Test scope

Eleven original tests are written, not run. They cover independent unit
quarter controls/tangents, conservative-cap refusal, literal mm budgets at
small and5000mm radii, joins, reverse/translation, repeatability, budgets,
hostile inputs and precision collapse. The boundary increment covers refusal of
the least-positive-subnormal allowance at cap64, subnormal radius/sweep rejection
by existing Seg2 admission, and required ordinary-radius success with a subnormal
centre and finite f64::MAX allowance. Independent Bernstein radial samples
supplement the derivation; they are not a between-sample error certificate.
The allocation-failure branch is fallible by source, not exercised by a
fabricated out-of-memory test. Compiler, formatter, Clippy, wasm, runtime
are pending. Kernel's bounded independent source review5c60c859 found no
demonstrated defect in the admitted domain; this is not formal verification or
executed acceptance. Final review of the three boundary tests remains pending.

Production paper.rs is byte-identical before/after. Only the existing
test file has a path inclusion for the private candidate. The eventual
module declaration is shown in a separate pending-adapter patch, NOT
integrated into the after source. Its signature remains internal/proposed.
