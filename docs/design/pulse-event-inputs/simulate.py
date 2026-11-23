"""Brute-force check of event-input rules for one-frame pulses (issue #800).

Run: python3 -I docs/design/pulse-event-inputs/simulate.py > docs/design/pulse-event-inputs/results.txt

Standard library only; deterministic output. It compares, per frame, how many times a Modelica
`when c` fires at the frame's instant with whether each engine rule fires, for every Boolean
function `c` of a small set of leaves (a "cone": stateless logic, given as a truth table, so all
2^(2^n) cones of n leaves are covered), and for the named shapes of the design note.

THE MODELICA MODEL ("snapped": every event of the frame interval (t(k-1), t(k)] at t(k)).
At the instant of frame k the event iteration is a sequence of values, iteration 0 being the left
limit (the continuous-time value just before t(k)) and iteration j >= 1 being computed from the
values of iteration j - 1 as pre() (MLS 3.6 §8.6: after each iteration pre(v) := v, repeated until
nothing changes). A `when c` fires once per false-to-true step of c along that sequence (§8.3.5,
"becomes true"). Per node:
  host level x   iteration 0: x(k-1); j >= 1: x(k)  (inputs held between frames)
  sample S       iteration 0: false; j >= 1: S(k)  (true through EVERY iteration of its instant and
                 false in continuous time, where no `when` sees the fall: what OpenModelica 1.25.1
                 does: `when not sample(0, 1)` never fires, om/results.txt)
  change(x)      iteration 0: false; j >= 1: x_j <> x_(j-1)   (§3.7.5; CDL/Logical/Change.mo:15)
  edge(x)        iteration 0: false; j >= 1: x_j and not x_(j-1)  (§3.7.5; CDL/Logical/Edge.mo:17;
                 FallingEdge.mo:21 is edge(not x))
  pre(x)         iteration 0: x_0; j >= 1: x_(j-1)              (CDL/Logical/Pre.mo:17)
  not/and/or/xor pointwise in each iteration
THE ENGINE MODEL (HostTick v1 at 128f7a8d): frame values, each block from this frame's input and
the previous frame's: change(x)(k) = x(k) <> x(k-1), edge(x)(k) = x(k) and not x(k-1),
pre(x)(k) = x(k-1) (blocks/logical_latch.rs:112, blocks/logical.rs:434, Pre at logical.rs:342),
SampleTrigger(k) = S(k), logic pointwise; v(k) is the cone on frame k's leaf values.
RULES (each fires at most once per frame):
  today       v(k) and not v(k-1)
  a           reviewer's labels, semantic closure: the cone is a pulse when it is false whenever
              every pulse leaf is false (covers And-with-a-pulse and Or-of-pulses); pulse leaves:
              change, edge, sample, and pre of a pure pulse. Pulse cone: v(k); else today
  a-prim      as a, pulse leaves only PRIMITIVE ones (change/edge of a pulse-free, pre-free net,
              and sample), and a cone holding any other pre or change/edge leaf uses today's rule
  f           the first draft's settled value: s = cone with change/edge/sample/pre-of-pulse
              leaves false; fire = (v(k) and not s(k-1)) or (not v(k) and s(k))
  g           corrected: a cone with a pre(...) leaf or a non-primitive change/edge leaf fires by
              today's rule; otherwise w = cone with primitive pulses and samples false, s = cone
              with primitive pulses false and samples at their frame value, and
              fire = (not w(k-1) and v(k)) or (not v(k) and s(k))
Every host and sample sequence of a few frames is enumerated from a settled frame 0 (no pulse,
no sample, pre(x) = x); the Tally class says how a fire is matched to a Modelica event (on time,
one frame late, or invented).

FRAME 0 (added in the review of GitHub #805, after the pinned-class probes InitNegatedPilot and
InitMixturesPilot). THIS MODEL IS FITTED TO THOSE PROBES, NOT DERIVED INDEPENDENTLY: it is the
sequence the probes' observations fit, written so the rules can be compared over every cone. At
the initial time the leaves take, in order:
  initialisation  hosts x(0); change(x) = x(0) <> pre_u_start, edge(x) = x(0) and not
                  pre_u_start (pre_u_start = false, CDL/Logical/Change.mo, Edge.mo); pre(x) =
                  pre_u_start; samples false ("The start value of the special functions initial,
                  terminal, and sample is false", MLS 3.6 section 8.3.5.1)
  event iteration pre(v) := v until nothing changes, samples still false ("pre(vi) := vi must be
                  set and an event iteration at the initial time must follow", section 8.6)
  sample event    the samples due at the initial time true ("Returns true and triggers time events
                  at time instants", operator 3.23), then pre(v) := v until nothing changes with
                  the samples held true (what OpenModelica 1.25.1 does through an instant's
                  iterations, om/results.txt; the specification has sample true "only during the
                  first event iteration", which no cone of these leaves can tell apart)
A `when c` that is not active at initialisation (section 8.6: "The equations of a when-clause are
active during initialization ... if and only if they are explicitly enabled with initial()")
fires once per false-to-true step of c after initialisation. For primitive pulses and samples
these three states are the engine's i, w and s.
"""

import itertools


class N:
    """An expression node: kind in host, sample, not, and, or, xor, change, edge, pre."""

    def __init__(self, kind, *args, name=None):
        self.kind, self.args, self.name = kind, args, name

    def __repr__(self):
        if self.kind in ("host", "sample"):
            return self.name
        inner = ", ".join(repr(a) for a in self.args)
        label = {"change": "Change", "edge": "Edge", "pre": "Pre", "not": "Not", "and": "And",
                 "or": "Or", "xor": "Xor"}[self.kind]
        return f"{label}({inner})"


def host(n):
    return N("host", name=n)


def sample(n):
    return N("sample", name=n)


def Not(x):
    return N("not", x)


def And(*x):
    return N("and", *x)


def Or(*x):
    return N("or", *x)


def Xor(x, y):
    return N("xor", x, y)


def Change(x):
    return N("change", x)


def Edge(x):
    return N("edge", x)


def FallingEdge(x):
    return N("edge", Not(x))


def Pre(x):
    return N("pre", x)


LOGIC = ("not", "and", "or", "xor")


def logic(kind, vals):
    if kind == "not":
        return not vals[0]
    if kind == "and":
        return all(vals)
    if kind == "or":
        return any(vals)
    return vals[0] != vals[1]


def nodes(x, out):
    for a in x.args:
        nodes(a, out)
    if x not in out:
        out.append(x)
    return out


def has_kind(x, kinds):
    return x.kind in kinds or any(has_kind(a, kinds) for a in x.args)


def leaf_class(x):
    """host | sample | prim (primitive pulse) | nonprim (pulse of a pulse or of a pre) |
    prepulse (pre of a pure pulse) | prelevel (pre of anything else)."""
    if x.kind in ("host", "sample"):
        return x.kind
    if x.kind in ("change", "edge"):
        return "nonprim" if has_kind(x.args[0], ("change", "edge", "sample", "pre")) else "prim"
    if x.kind == "pre":
        return "prepulse" if pure_pulse(x.args[0]) else "prelevel"
    raise ValueError(x)


def pure_pulse(x):
    """False whenever every change/edge/sample inside it is false (checked by enumeration)."""
    ls = cone_leaves(x)
    pul = [i for i, l in enumerate(ls) if l.kind in ("change", "edge", "sample")]
    if not pul:
        return False
    lev = [i for i in range(len(ls)) if i not in pul]
    for bits in itertools.product((False, True), repeat=len(lev)):
        env = dict(zip([ls[i] for i in lev], bits))
        env.update({ls[i]: False for i in pul})
        if eval_cone(x, env):
            return False
    return True


def cone_leaves(x, out=None):
    out = [] if out is None else out
    if x.kind in LOGIC:
        for a in x.args:
            cone_leaves(a, out)
    elif x not in out:
        out.append(x)
    return out


def eval_cone(x, env):
    if x.kind in LOGIC:
        return logic(x.kind, [eval_cone(a, env) for a in x.args])
    return env[x]


def engine_value(x, k, hs, memo, settled=False):
    """Frame value of node x at frame k. Before frame 0 every block holds its start value
    (false). With settled=True frame 0 is a quiet, settled state instead: hosts at their frame-0
    value, every change, edge and sample false, pre(x) equal to x (the brute force starts there,
    so that no start-up artefact counts as a difference)."""
    if k < 0:
        return False
    key = (id(x), k)
    if key in memo:
        return memo[key]
    if settled and k == 0:
        if x.kind == "host":
            v = hs[x.name][0]
        elif x.kind in ("sample", "change", "edge"):
            v = False
        elif x.kind in LOGIC:
            v = logic(x.kind, [engine_value(a, 0, hs, memo, settled) for a in x.args])
        else:
            v = engine_value(x.args[0], 0, hs, memo, settled)
        memo[key] = v
        return v
    if x.kind in ("host", "sample"):
        v = hs[x.name][k]
    elif x.kind in LOGIC:
        v = logic(x.kind, [engine_value(a, k, hs, memo, settled) for a in x.args])
    elif x.kind == "change":
        v = (engine_value(x.args[0], k, hs, memo, settled)
             != engine_value(x.args[0], k - 1, hs, memo, settled))
    elif x.kind == "edge":
        v = (engine_value(x.args[0], k, hs, memo, settled)
             and not engine_value(x.args[0], k - 1, hs, memo, settled))
    else:  # pre
        v = engine_value(x.args[0], k - 1, hs, memo, settled)
    memo[key] = v
    return v


def modelica_iterations(leaves, old, new, samp):
    """Leaf-value tuples of the event iteration at one instant: [left limit, iteration 1, ...]."""
    allnodes = []
    for l in leaves:
        nodes(l, allnodes)
    prev = {}
    for x in allnodes:  # iteration 0, children first
        if x.kind == "host":
            prev[x] = old[x.name]
        elif x.kind in ("sample", "change", "edge"):
            prev[x] = False
        elif x.kind in LOGIC:
            prev[x] = logic(x.kind, [prev[a] for a in x.args])
        else:
            prev[x] = prev[x.args[0]]
    seq = [tuple(prev[l] for l in leaves)]
    for _ in range(32):
        cur = {}
        for x in allnodes:
            if x.kind == "host":
                cur[x] = new[x.name]
            elif x.kind == "sample":
                cur[x] = samp.get(x.name, False)
            elif x.kind in LOGIC:
                cur[x] = logic(x.kind, [cur[a] for a in x.args])
            elif x.kind == "change":
                cur[x] = cur[x.args[0]] != prev[x.args[0]]
            elif x.kind == "edge":
                cur[x] = cur[x.args[0]] and not prev[x.args[0]]
            else:
                cur[x] = prev[x.args[0]]
        if cur == prev:
            return seq
        seq.append(tuple(cur[l] for l in leaves))
        prev = cur
    raise RuntimeError("event iteration did not settle")


def idx(t):
    return sum(1 << i for i, b in enumerate(t) if b)


def popcount(x):
    return x.bit_count()


class Planes:
    """Bit-parallel truth tables: bit c of T[x] is the value of cone c at leaf assignment x."""

    def __init__(self, n):
        self.n, self.width = n, 1 << (1 << n)
        self.mask = (1 << self.width) - 1
        self.T = []
        for x in range(1 << n):
            bits, block = 0, 1 << x  # cone c is true at x iff bit x of c is set
            period = block << 1
            run = (1 << block) - 1
            for start in range(block, self.width, period):
                bits |= run << start
            self.T.append(bits)

    def false_on(self, xs):
        acc = self.mask
        for x in xs:
            acc &= ~self.T[x] & self.mask
        return acc

    def depends(self, bit):
        acc = 0
        for x in range(1 << self.n):
            acc |= self.T[x] ^ self.T[x ^ (1 << bit)]
        return acc


RULES = ("today", "a", "a-prim", "f", "g")


def rule_masks(leaves):
    cls = [leaf_class(l) for l in leaves]
    bit = lambda kinds: sum(1 << i for i, c in enumerate(cls) if c in kinds)  # noqa: E731
    return cls, {
        "a": bit(("prim", "nonprim", "sample", "prepulse")),
        "a-prim": bit(("prim", "sample")),
        "f": bit(("prim", "nonprim", "sample", "prepulse")),
        "g_pulse": bit(("prim",)),
        "g_sample": bit(("sample",)),
        "g_fallback": bit(("nonprim", "prepulse", "prelevel")),
    }


class Tally:
    """Matches each rule's fires against Modelica's events, per cone, along one sequence.

    A rule fire at frame k is matched first to an unmatched Modelica event of frame k - 1 (the
    fire is LATE), then to one of frame k (ON TIME); if neither exists it is INVENTED. A Modelica
    event of frame k - 1 still unmatched after frame k is MISSED. The last frame's unmatched
    events are not counted as missed (the sequence ends)."""

    def __init__(self, P):
        self.P = P
        self.invented = self.missed = self.late = 0
        self.invented_pairs = 0

    def run(self, frames):
        """frames: list of (modelica_ones, modelica_twos, rule_fire) planes, in order."""
        M = self.P.mask
        p1 = p2 = 0  # pending Modelica events from the previous frame (>= 1, >= 2)
        inv_seq = 0
        for m1, m2, r in frames:
            late = r & p1
            rem = r & ~p1 & M
            now = rem & m1
            inv = rem & ~m1 & M
            # pending after a late match drops by one; whatever is left expires now
            left1 = (p1 & ~late & M) | (p2 & late)
            self.missed |= left1
            self.late |= late
            inv_seq |= inv
            # this frame's events, minus the one matched on time, become pending
            p1 = (m1 & ~now & M) | (m2 & now)
            p2 = m2 & ~now & M
        self.invented |= inv_seq
        self.invented_pairs += inv_seq.bit_count()


def sequences(hosts, samples, length):
    """Every host pattern over frames 0 .. length-1 and every sample pattern with no sample at
    frame 0 (frame 0 is the settled start)."""
    bits = len(hosts) * length + len(samples) * (length - 1)
    for pattern in itertools.product((False, True), repeat=bits):
        hs, i = {}, 0
        for h in hosts:
            hs[h] = pattern[i:i + length]
            i += length
        for sname in samples:
            hs[sname] = (False,) + pattern[i:i + length - 1]
            i += length - 1
        yield hs


def rule_fires(P, m, full, pulse_cone, fallback, xv, xp):
    """Fire planes of every rule at one frame, from the leaf assignments at frames k and k-1."""
    V, Vp = P.T[xv], P.T[xp]
    today = V & ~Vp & P.mask
    fire = {"today": today}
    fire["a"] = (pulse_cone["a"] & V) | (~pulse_cone["a"] & today & P.mask)
    prim = pulse_cone["a-prim"] & ~fallback & P.mask
    fire["a-prim"] = (prim & V) | (~prim & today & P.mask)
    S, Sp = P.T[xv & ~m["f"] & full], P.T[xp & ~m["f"] & full]
    fire["f"] = (V & ~Sp & P.mask) | (~V & S & P.mask)
    W = P.T[xp & ~(m["g_pulse"] | m["g_sample"]) & full]
    G = P.T[xv & ~m["g_pulse"] & full]
    g = (~W & V & P.mask) | (~V & G & P.mask)
    fire["g"] = (fallback & today) | (~fallback & g & P.mask)
    return fire


def modelica_planes(P, seq):
    ones = twos = 0
    prevp = P.T[idx(seq[0])]
    for t in seq[1:]:
        cur = P.T[idx(t)]
        rise = ~prevp & cur & P.mask
        twos |= ones & rise
        ones |= rise
        prevp = cur
    return ones, twos


def setup(leaves):
    n = len(leaves)
    P = Planes(n)
    cls, m = rule_masks(leaves)
    full = (1 << n) - 1
    pulse_cone = {r: P.false_on([x for x in range(1 << n) if x & m[r] == 0])
                  for r in ("a", "a-prim")}
    fallback = 0
    for i in range(n):
        if m["g_fallback"] >> i & 1:
            fallback |= P.depends(i)
    return P, cls, m, full, pulse_cone, fallback


def brute(leaves, hosts, samples, length):
    """Snapped Modelica against every rule, over every sequence of the given length."""
    P, cls, m, full, pulse_cone, fallback = setup(leaves)
    tallies = {r: Tally(P) for r in RULES}
    exact = dict.fromkeys(RULES, 0)  # cones whose fire equals Modelica's count at every frame
    cache = {}
    count = 0
    for hs in sequences(hosts, samples, length):
        count += 1
        memo = {}
        xs = [idx([engine_value(l, k, hs, memo, True) for l in leaves]) for k in range(length)]
        per_rule = {r: [] for r in RULES}
        for k in range(1, length):
            key = (tuple(hs[h][k - 1] for h in hosts), tuple(hs[h][k] for h in hosts),
                   tuple(hs[sn][k] for sn in samples))
            if key not in cache:
                seq = modelica_iterations(leaves, dict(zip(hosts, key[0])),
                                          dict(zip(hosts, key[1])), dict(zip(samples, key[2])))
                cache[key] = modelica_planes(P, seq)
            m1, m2 = cache[key]
            fire = rule_fires(P, m, full, pulse_cone, fallback, xs[k], xs[k - 1])
            for r in RULES:
                per_rule[r].append((m1, m2, fire[r]))
                exact[r] |= (fire[r] ^ m1) | m2
        for r in RULES:
            tallies[r].run(per_rule[r])
    return {"cones": P.width, "sequences": count, "classes": cls,
            "invented": {r: tallies[r].invented.bit_count() for r in RULES},
            "missed": {r: tallies[r].missed.bit_count() for r in RULES},
            "late": {r: tallies[r].late.bit_count() for r in RULES},
            "exact": {r: P.width - exact[r].bit_count() for r in RULES},
            "pairs": {r: tallies[r].invented_pairs for r in RULES}}


def unsnapped(leaves, hosts, length):
    """Leaves built from host levels only. Inside a frame each changed host changes at its own
    sub-instant, in every order, or all together (the snapped case); Modelica's events of the
    frame are the sum over its sub-instants. Returns, per rule, the cones with an invented event
    on some sequence, and the (cone, sequence) pairs with one."""
    P, cls, m, full, pulse_cone, fallback = setup(leaves)
    inv = {r: 0 for r in ("today", "g")}
    pairs = {r: 0 for r in ("today", "g")}
    g_not_today = 0
    cache = {}
    for hs in sequences(hosts, [], length):
        memo = {}
        xs = [idx([engine_value(l, k, hs, memo, True) for l in leaves]) for k in range(length)]
        fires = [rule_fires(P, m, full, pulse_cone, fallback, xs[k], xs[k - 1])
                 for k in range(1, length)]
        options = []
        for k in range(1, length):
            changed = [h for h in hosts if hs[h][k] != hs[h][k - 1]]
            orders = [(tuple(changed),)]
            if len(changed) > 1:
                orders += [tuple((h,) for h in perm) for perm in itertools.permutations(changed)]
            planes = []
            for order in orders:
                ckey = (tuple(hs[h][k - 1] for h in hosts), order,
                        tuple(hs[h][k] for h in hosts))
                if ckey not in cache:
                    state = {h: hs[h][k - 1] for h in hosts}
                    ones = twos = 0
                    for group in order:
                        new = dict(state)
                        for h in group:
                            new[h] = hs[h][k]
                        o, t = modelica_planes(P, modelica_iterations(leaves, state, new, {}))
                        twos |= (ones & o) | t
                        ones |= o
                        state = new
                    cache[ckey] = (ones, twos)
                planes.append(cache[ckey])
            options.append(planes)
        for choice in itertools.product(*options):
            seq_inv = {}
            for r in ("today", "g"):
                t = Tally(P)
                t.run([(m1, m2, f[r]) for (m1, m2), f in zip(choice, fires)])
                seq_inv[r] = t.invented
                inv[r] |= t.invented
                pairs[r] += t.invented_pairs
            g_not_today |= seq_inv["g"] & ~seq_inv["today"] & P.mask
    return ({r: v.bit_count() for r, v in inv.items()}, (inv["today"] & inv["g"]).bit_count(),
            g_not_today.bit_count(), pairs)


def trace(shape, hs, frames):
    """Event counts over frames 1 .. frames-1 for one named shape."""
    leaves = cone_leaves(shape)
    cls, m = rule_masks(leaves)
    full = (1 << len(leaves)) - 1
    memo = {}
    hostnames = sorted({x.name for l in leaves for x in nodes(l, []) if x.kind == "host"})
    sampnames = sorted({x.name for l in leaves for x in nodes(l, []) if x.kind == "sample"})

    def cone(x):
        return eval_cone(shape, {l: bool(x >> i & 1) for i, l in enumerate(leaves)})

    def pulse_cone(mask):
        return all(not cone(x) for x in range(full + 1) if x & mask == 0)

    fallback = any(m["g_fallback"] >> i & 1 and any(cone(x) != cone(x ^ (1 << i))
                                                     for x in range(full + 1))
                   for i in range(len(leaves)))
    counts = dict.fromkeys(("modelica",) + RULES, 0)
    for k in range(1, frames):
        xv = idx([engine_value(l, k, hs, memo) for l in leaves])
        xp = idx([engine_value(l, k - 1, hs, memo) for l in leaves])
        seq = modelica_iterations(leaves, {h: hs[h][k - 1] for h in hostnames},
                                  {h: hs[h][k] for h in hostnames},
                                  {s: hs[s][k] for s in sampnames})
        vals = [cone(idx(t)) for t in seq]
        counts["modelica"] += sum(1 for p, c in zip(vals, vals[1:]) if c and not p)
        v, vp = cone(xv), cone(xp)
        today = v and not vp
        counts["today"] += today
        counts["a"] += v if pulse_cone(m["a"]) else today
        counts["a-prim"] += v if pulse_cone(m["a-prim"]) and not fallback else today
        s, sp = cone(xv & ~m["f"] & full), cone(xp & ~m["f"] & full)
        counts["f"] += (v and not sp) or (not v and s)
        w = cone(xp & ~(m["g_pulse"] | m["g_sample"]) & full)
        sg = cone(xv & ~m["g_pulse"] & full)
        counts["g"] += today if fallback else ((not w and v) or (not v and sg))
    return counts, fallback


def modelica_initial(leaves, x0, samp):
    """Leaf-value tuples at the initial time: [initialisation, ..., settled] (the fitted model)."""
    allnodes = []
    for l in leaves:
        nodes(l, allnodes)
    prev = {}
    for x in allnodes:  # initialisation, children first
        if x.kind == "host":
            prev[x] = x0[x.name]
        elif x.kind == "sample":
            prev[x] = False
        elif x.kind in LOGIC:
            prev[x] = logic(x.kind, [prev[a] for a in x.args])
        elif x.kind == "change":
            prev[x] = prev[x.args[0]] != False  # noqa: E712 (pre_u_start)
        elif x.kind == "edge":
            prev[x] = prev[x.args[0]] and not False
        else:
            prev[x] = False
    seq = [tuple(prev[l] for l in leaves)]
    for sampling in (False, True):
        for _ in range(32):
            cur = {}
            for x in allnodes:
                if x.kind == "host":
                    cur[x] = x0[x.name]
                elif x.kind == "sample":
                    cur[x] = sampling and samp.get(x.name, False)
                elif x.kind in LOGIC:
                    cur[x] = logic(x.kind, [cur[a] for a in x.args])
                elif x.kind == "change":
                    cur[x] = cur[x.args[0]] != prev[x.args[0]]
                elif x.kind == "edge":
                    cur[x] = cur[x.args[0]] and not prev[x.args[0]]
                else:
                    cur[x] = prev[x.args[0]]
            if cur == prev:
                break
            seq.append(tuple(cur[l] for l in leaves))
            prev = cur
        else:
            raise RuntimeError("initial event iteration did not settle")
    return seq


def initial_frame(leaves, hosts, samples):
    """Every cone of `leaves` at the initial frame, every host value and sample on or off the
    grid: how many (cone, start) pairs each rule fires exactly as the fitted Modelica model."""
    n = len(leaves)
    pulse = [l.kind in ("change", "edge") for l in leaves]
    samp = [l.kind == "sample" for l in leaves]
    counts = {"none (#768)": 0, "not i and s": 0, "i -> w -> s": 0}
    differ = {"not i and s": [], "i -> w -> s": []}
    starts = 0
    for hv in itertools.product([False, True], repeat=len(hosts)):
        for sv in itertools.product([False, True], repeat=len(samples)):
            x0, sd = dict(zip(hosts, hv)), dict(zip(samples, sv))
            seq = modelica_initial(leaves, x0, sd)
            v = seq[0]  # the leaves at initialisation are the engine's i
            i = idx(v)
            w = idx(tuple(False if (p or q) else b for b, p, q in zip(v, pulse, samp)))
            s = idx(tuple(False if p else (sd[l.name] if q else b)
                          for b, p, q, l in zip(v, pulse, samp, leaves)))
            states = [idx(t) for t in seq]
            starts += 1
            for c in range(1 << (1 << n)):
                bit = lambda t: (c >> t) & 1 == 1  # noqa: E731
                om = sum(1 for a, b in zip(states, states[1:]) if bit(b) and not bit(a))
                fires = {
                    "none (#768)": 0,
                    "not i and s": int(bit(s) and not bit(i)),
                    "i -> w -> s": int(bit(w) and not bit(i)) + int(bit(s) and not bit(w)),
                }
                for rule, f in fires.items():
                    if f == om:
                        counts[rule] += 1
                    elif rule in differ and len(differ[rule]) < 3:
                        differ[rule].append((c, hv, sv))
    return counts, differ, starts * (1 << (1 << n))


def main():
    a, b, S = host("a"), host("b"), sample("S")
    print("# Issue #800: brute force over every cone (Boolean function) of each leaf set")
    print("# Cones out of 2^(2^n), over every host and sample sequence of the given length (frame 0")
    print("# a settled start). exact: the rule fires exactly when, and as often as, snapped Modelica")
    print("# at every frame. invent: some fire matches no Modelica event of its frame or the frame")
    print("# before. miss: some Modelica event is matched by no fire of its frame or the next. late:")
    print("# some fire matches only the previous frame's event.")
    sets = [
        ("item 1: Change(a), S, a, b", [Change(a), S, a, b], ["a", "b"], ["S"], 5),
        ("item 2: Change(a), Pre(a), b", [Change(a), Pre(a), b], ["a", "b"], [], 6),
        ("item 2: Change(a), a, Pre(b)", [Change(a), a, Pre(b)], ["a", "b"], [], 6),
        ("item 3: Change(a), Pre(Change(a)), b", [Change(a), Pre(Change(a)), b], ["a", "b"], [], 6),
        ("item 3: Change(a), Change(Edge(a)), b", [Change(a), Change(Edge(a)), b], ["a", "b"], [],
         6),
        ("item 3: Pre(Change(a)), a, b", [Pre(Change(a)), a, b], ["a", "b"], [], 6),
        ("item 3: Change(Edge(a)), a, b", [Change(Edge(a)), a, b], ["a", "b"], [], 6),
        ("primitive: Change(a), Change(b), a, b", [Change(a), Change(b), a, b], ["a", "b"], [], 6),
        ("primitive: Edge(a), FallingEdge(b), a, b", [Edge(a), FallingEdge(b), a, b], ["a", "b"],
         [], 6),
        ("primitive: Change(a), S, a", [Change(a), S, a], ["a"], ["S"], 6),
    ]
    print(f"{'leaf set':42} {'cones':>6} {'seqs':>6} | "
          + " | ".join(f"{r:>23}" for r in RULES))
    print(f"{'':42} {'':>6} {'':>6} | " + " | ".join(f"{'exact/invent/miss/late':>23}" for _ in RULES))
    for label, leaves, hosts, samples, length in sets:
        r = brute(leaves, hosts, samples, length)
        cells = " | ".join(
            f"{r['exact'][x]:>5}/{r['invented'][x]:>5}/{r['missed'][x]:>5}/{r['late'][x]:>5}"
            for x in RULES)
        print(f"{label:42} {r['cones']:>6} {r['sequences']:>6} | {cells}")
    print()
    print("# Snapping (item 7): leaves Change(a), Change(b), a, b, 5-frame sequences. Inside a frame")
    print("# each changed level changes at its own sub-instant, in either order, or both together;")
    print("# Modelica's events of the frame are summed over its sub-instants. 'invents' as above.")
    inv, both, g_only, pairs = unsnapped([Change(a), Change(b), a, b], ["a", "b"], 5)
    print(f"cones where today invents on some sequence: {inv['today']}")
    print(f"cones where g invents on some sequence:     {inv['g']}")
    print(f"cones where both invent:                    {both}")
    print(f"cones where g invents on a sequence where today does not: {g_only}")
    print(f"invented (cone, sequence) pairs: today {pairs['today']}, g {pairs['g']}")
    print()
    print("# The initial frame (review of GitHub #805): every cone of Change(a), S, a, b, every a(0),")
    print("# b(0), with S's instant at the start or not, against the FITTED Modelica model of the")
    print("# initial time (initialisation -> event iteration -> sample event; see the docstring).")
    print("# Exact (cone, start) pairs per rule, out of the total:")
    counts, differ, total = initial_frame([Change(a), S, a, b], ["a", "b"], ["S"])
    for rule, n in counts.items():
        print(f"  {rule:14} {n:>7} / {total}")
    for rule, ex in differ.items():
        print(f"  first differences, {rule}: "
              + ("none" if not ex else "; ".join(f"cone {c:#06x} a,b={hv} S={sv}" for c, hv, sv in ex)))
    print()
    print("# The shapes of the note's section 1.3: u changes at 2, 3, 5, 7, 8, 9 s; lvl on [4, 8);")
    print("# lvl2 from 4 s (3.5 s in OpenModelica, shown at the 4 s frame); b on [2, 7); 1 s frames;")
    print("# events counted at frames 1 to 9 (frame 0 is outside this script)")
    F = 10
    useq = [k in (2, 5, 6, 8) for k in range(F)]
    hs = {"u": useq, "u2": useq, "lvl": [4 <= k < 8 for k in range(F)], "lvl2": [k >= 4 for k in range(F)],
          "b": [2 <= k < 7 for k in range(F)], "S1": [True] * F,
          "S14": [False, False, True, True, False, True, True, True, False, True],
          "S2": [k % 2 == 0 for k in range(F)]}
    u, lvl, lvl2, bb = host("u"), host("lvl"), host("lvl2"), host("b")
    shapes = [
        ("1 Change(u)", Change(u)),
        ("2 Not(Change(u))", Not(Change(u))),
        ("3 And(Change(u), lvl)", And(Change(u), lvl)),
        ("4 Or(Change(u), lvl)", Or(Change(u), lvl)),
        ("5 Or(Change(u), Change(u2)), u2 = u", Or(Change(u), Change(host("u2")))),
        ("6 Xor(u, Pre(u))", Xor(u, Pre(u))),
        ("7 And(u, Not(Pre(u)))", And(u, Not(Pre(u)))),
        ("8 Edge(Not(Change(u)))", Edge(Not(Change(u)))),
        ("9 Edge(Change(u))", Edge(Change(u))),
        ("10 Pre(Change(u))", Pre(Change(u))),
        ("11 Or(Change(u), lvl2)", Or(Change(u), lvl2)),
        ("13 SampleTrigger(1 s)", sample("S1")),
        ("14 SampleTrigger(1.4 s)", sample("S14")),
        ("15 SampleTrigger(2 s)", sample("S2")),
        ("16 Not(SampleTrigger(1 s))", Not(sample("S1"))),
        ("17 Or(Change(u), Pre(b))", Or(Change(u), Pre(bb))),
        ("18 Not(Or(Change(u), Pre(u)))", Not(Or(Change(u), Pre(u)))),
        ("19 Or(Change(u), Pre(Change(u)))", Or(Change(u), Pre(Change(u)))),
        ("20 Change(Edge(u))", Change(Edge(u))),
        ("21 And(Change(u), Edge(Not(Change(u))))", And(Change(u), Edge(Not(Change(u))))),
    ]
    print(f"{'shape':42} {'Modelica':>8} " + " ".join(f"{r:>7}" for r in RULES) + "  g falls back")
    for label, shape in shapes:
        c, fb = trace(shape, hs, F)
        print(f"{label:42} {c['modelica']:>8} " + " ".join(f"{c[r]:>7}" for r in RULES)
              + f"  {'yes' if fb else 'no'}")


if __name__ == "__main__":
    main()
