# V2 error argument and unit composition

This is original source reasoning, NOT an executed formal proof or runtime result. The v1 derivation is retained byte-for-byte in v1/DERIVATION.md. Existing construction/provenance and its Bernstein-control argument remain; no external implementation is copied.

## Extension from a quarter to a half turn

For an exact-real subarc theta with0<abs(theta)<=pi, beta=abs(theta)/4 lies in(0,pi/4]. The unchanged tangent/midpoint cubic has

||B(t)||^2/r^2 -1 =16 sin(beta)^6/cos(beta)^2 *t^2(1-t)^2(2t-1)^2.

With z=t(1-t), z^2(1-4z)<=1/108, so radial deviation is at most
r*(2/27)*sin(beta)^6/cos(beta)^2.

The ideal cubic stays in the circular sector: in the symmetric coordinates its endpoint controls are(c,-s),(c+k*s,-s+k*c),(c+k*s,s-k*c),(c,s), with c=cos(theta/2)>=0 and k=(4/3)tan(theta/4). The sector is convex through a half turn; interior x is positive, including at theta=pi. Its continuous argument covers the arc's endpoint angles. Radial projection in both directions gives the Hausdorff bound. The negative sweep is a reflection.

Because beta<=pi/4<1,
sin(beta)<=beta and cos(beta)^2>=1-beta^2>0.
The existing larger r*(2/27)*beta^6/(1-beta^2) therefore remains valid; its proof does not require beta<=pi/8. At a half turn this deliberately conservative bound may need more pieces than the tight formula.

The angle endpoint range becomes[-2pi,2pi]. The same24-term Taylor sums and outward remainder recurrences are retained, with remainder bounds |x|^49/49! and |x|^48/48!. Those derivative bounds are global, so no new period-reduction or libm assumption is introduced. Wider intervals can refuse certification rather than relax error.

Each stored control remains inside its computed enclosure; its max L1 displacement bounds the cubic perturbation because Bernstein weights are nonnegative and sum to1. Adding the largest control uncertainty to the outward ideal bound proves the result's error_bound in the arc's own units.

## Finite generic input is not authored paper admission

The converter checks finite center, finite radius>0, start in[-PI,PI],0<abs(sweep)<=PI, finite error>0 and cap1..64. It deliberately does not call Seg2::check(MAX_COORDINATE_MM): that validator fixes authored mm geometry, whereas Piece::seg returns grid steps. A finite positive subnormal radius is not an invalid radius; representability, collapse or a failed certificate can still refuse it. Zero and negative radius refuse before arithmetic.

No source coordinate/radius clamp, saturation, normalization or catch-and-return-success exists. Arithmetic overflow/nonfinite enclosure, precision collapse, allocation failure and unqualified error discard the local result. The search still has at most64 trials/2080 attempted pieces and64 logical temporary pieces, with unchanged fixed scalar recurrence bounds. This is not a measured memory/time claim.

## What the writer must prove

If the helper returns a stored cubic C with d_H(C,A)<=E and the writer changes each stored control Pi to Qi, Bernstein convexity gives a second displacement bound
D=max_i ||Qi-Pi|| (or a conservative L1 bound).
Then d_H(Q,A)<=E+D. This includes replacements of the first or last endpoint: it is not enough to call an arbitrary replacement 'rounding'.

For nearest-grid rounding alone, D<=sqrt(2)/2 grid steps. When the last endpoint is forcibly set to the walked to, the adapter must bound that actual displacement too; the generic helper cannot certify the caller's chosen target. Piece::seg's reconstructed f64 center/start/sweep need not make its exact-real endpoint identical to the integer to. No completed end-replacement proof or adapter is claimed here.

On the actual TEN_NANOMETRES grid, one millimetre is100000 steps and ARC_TOLERANCE_MM1e-4 is10 steps. The writer reserves its rounding/replacement allowance before asking the helper, and converts the final report to mm exactly once. SVG/shared-walk records and all existing writer code remain unchanged in this packet.

## Independent expectations

Half-turn controls and midpoint are literal unit-circle geometry. The slightly-over-quarter endpoint sign/range follows delta=1/4096 and delta-delta^3/6<=sin(delta)<=delta. The quarter certificate floor follows the analytic maximum z=1/6; the known deviation exceeds0.00027 radius, so a false zero certificate fails. Tests do not use dense radial samples to supply a certificate or record writer output as the expected answer.

The thirteen written helper tests and the separate two writer fixtures are NOT COMPILED and NOT RUN.
