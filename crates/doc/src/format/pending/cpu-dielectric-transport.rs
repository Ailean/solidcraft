//! CPU render setups save their explicit dielectric transport choice. Missing or
//! older values retain LegacyBounded; its default is omitted. StrictTir preserves
//! deterministic internal reflection and refuses unresolved positive optical tails;
//! it does not make captured body bindings portable or add unsupported geometry.
//! No existing value needs migration or clamping. The last release refuses the
//! newer design format and asks for an update instead of losing the saved choice.
