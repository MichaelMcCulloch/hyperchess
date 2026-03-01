
rbtlong.com
Recursive DFS: 30 Million Stack Frames Deep | Robert Long
Robert Long
31–40 minutes

HPC C# Algorithms HPC DFS HPC Algorithms

820% faster. 30 million stack frames. 2x deeper than standard C#, 1,500x deeper than Java/Python. This is what C# can really do.

By Robert Long | January 6, 2025

I wanted to see how far I could push recursive Depth First Search—how deep into the stack could I go before it overflows? What I found was insane: 820% faster, 2x deeper than standard C#, and traversal depths of 30 MILLION stack frames. Read that again. Thirty million recursive calls.

If you've worked with Java, Node.js, Python, or Ruby, you know the pain: recursive algorithms hit stack overflow around 10,000 to 20,000 calls. The standard advice is always "convert to iterative"—but what if we could keep the elegance of recursion and go 1,500x deeper? That's not a typo. One thousand five hundred times deeper. While also being 8x faster.
The Algorithm: Pure Elegance

Let's start with the textbook DFS. It's beautiful in its simplicity—just 4 lines:

void Dfs(Node node, HashSet<Node> visited) {
    if (node == null || visited.Contains(node)) return;
    visited.Add(node);
    foreach (var neighbor in node.Neighbors) Dfs(neighbor, visited);
}

That's it. Guard clause, mark visited, recurse into neighbors. Every CS student learns this. It's clean, readable, and mathematically elegant.

The question is: how efficient can we make it?

Can we keep this recursive elegance while pushing the limits of speed and depth? Let's find out.
The Experiment: Standard vs. HPC Mode

I set up a controlled benchmark: traverse a deep linear graph (each node connects to exactly one neighbor) and measure two things:

    Maximum depth before stack overflow
    Traversal speed for equivalent depths

I ran two modes with maximum (~2GB) stack allocations:

    Standard GC Mode: Class-based nodes, object references, HashSet for visited tracking
    HPC Mode: Struct-based nodes, integer indices, bool[] for visited tracking

The Results: Mind-Blowing

Animated comparison showing HPC C# achieving 8x speed and 30M stack depth vs Standard C#

In standard mode—the way Java, Node.js, Python, and typical garbage-collected runtimes work—the stack overflows at around 5,000-10,000 calls with default stack. Even with a maximum ~2GB stack allocation, standard mode crashes at around 15-16 million frames.
Over 8x Faster + 30 Million Stack Frames

HPC mode is nearly an order of magnitude faster—and pushes to 30 million stack frames where standard mode crashes at 15 million. That's 2x deeper with the same stack allocation, and 1,500x deeper than default Java/Python/Node.js limits. Same algorithm. Same elegance. Completely different universe of performance.

The speed difference is staggering. At 5 million nodes, HPC mode completed in 202 ms while standard mode took 1,510 ms. At 10 million nodes: 414 ms vs 3,139 ms. At 15 million: 616 ms vs 5,023 ms. That's a consistent 8x speedup—and HPC keeps going to 30 million where standard crashes.
What About Go?

Go is often praised for its "goroutines" with growable stacks. Unlike traditional languages, Go doesn't pre-allocate a fixed stack—it starts small and grows as needed. Sounds perfect for deep recursion, right?

Here's the catch: When Go's stack grows, it actually allocates new memory on the heap and copies the entire stack over. This "copy-on-grow" behavior means:

    Each stack growth triggers a memory allocation
    The entire stack must be copied to the new location
    Pointers within the stack must be updated
    This overhead accumulates with depth

The result? Go can go deeper than standard GC languages, but it pays a significant speed penalty. HPC C# achieves both maximum depth and maximum speed because structs eliminate the need for this dance entirely.
Why Does Standard Mode Fail So Fast?

In languages like Java, Node.js, Python, and Ruby—and in standard garbage-collected mode—every object lives on the heap. When you create a node:

// Standard class-based node (heap-allocated)
class Node {
    int value;
    Node left, right;
    List<Node> neighbors;  // Object references
}

Each stack frame in standard mode carries:

    8 bytes per object reference (on 64-bit)
    Object header overhead (~24 bytes minimum per heap object)
    GC tracking metadata
    Virtual dispatch tables for polymorphism

This bloats every recursive call. The stack fills up fast, and the GC is constantly working to track all those heap references.
How HPC Mode Achieves 30 Million Depth
1. Struct-Based Nodes (Value Types)

Instead of heap-allocated classes, we use value types that live contiguously in memory:

// HPC struct-based node (stack/array allocated)
struct Node {
    int value;
    int left, right;       // Integer indices, not references
    List<int> neighbors;   // Indices into the node array
}

Key insight: By using int indices instead of object references, each node reference costs only 4 bytes instead of 8+, and there's no object header overhead. The nodes themselves are stored in a single contiguous array.
2. Cache-Friendly Memory Layout

Structs in an array are stored contiguously in memory. When the CPU fetches one node, nearby nodes are already in the cache. This eliminates cache misses that plague pointer-chasing in class-based graphs.
3. Minimal Stack Frame Size

The recursive function signature is lean:

void Dfs(
    Node[] nodes,           // Single array reference
    int currentIndex,       // 4-byte integer
    HashSet<int> visited    // Single reference to shared set
)

Compare this to standard mode where each call passes full object references and the runtime maintains additional metadata. The HPC stack frame is dramatically smaller.
4. Zero Allocation During Traversal

In standard mode, every node access potentially triggers GC checks and write barriers. In HPC mode, we're just indexing into an array—no allocations during traversal, no GC pauses, pure computation.

Note: The List<int> for neighbors is still a reference type (allocated once during graph construction), but iterating over it during traversal creates no new allocations. The integers inside are value types.
5. Native AOT Compilation

The benchmark was compiled with Native AOT (Ahead-of-Time compilation). This eliminates the JIT warm-up penalty entirely and produces a true native binary.

The advantages are massive:

    Instant startup — No JIT compilation at runtime, the app launches immediately
    Smaller footprint — Only the code you use gets compiled in, trimming unused dependencies
    Self-contained binary — Single executable, no .NET runtime required on the target machine
    Predictable performance — No JIT tier transitions, no warm-up variability

This is where C# leaves Java, Node.js, and Python in the dust. Those languages require:

    Java: JVM startup + JIT warm-up (seconds of cold start)
    Node.js: V8 engine initialization + module loading
    Python: Interpreter startup + bytecode compilation

With Native AOT, a C# CLI tool starts as fast as a C program. For serverless and edge computing, this difference is the gap between a usable product and an unusable one.
JIT vs AOT: Stack Frame Optimization

Key insight: HPC advantages appear in both JIT and AOT modes. In JIT mode, HPC achieves ~25% deeper recursion (12,050 vs 9,620). AOT compilation amplifies this further: Native AOT produces smaller, highly-optimized stack frames where HPC's value-type semantics truly shine. HPC AOT achieves 1.4x deeper than Standard AOT (19,430 vs 13,880) and 1.6x deeper than HPC JIT.
The Algorithm: Elegant and Fast

Here's the complete HPC DFS implementation:

public static void Dfs(
    this Node[] nodes,
    int currentIndex,
    bool[] visited)  // bool[] instead of HashSet - O(1) array access
{
    // Bounds check
    if (currentIndex < 0 || currentIndex >= nodes.Length)
        return;

    // Already visited? Single CPU instruction, no hashing
    if (visited[currentIndex])
        return;

    // Mark as visited
    visited[currentIndex] = true;

    // Recurse into neighbors (using integer indices)
    foreach (var neighborIndex in nodes[currentIndex].Neighbors)
        nodes.Dfs(neighborIndex, visited);
}

The elegance is preserved—it's still recursive, still readable—but the underlying data representation transforms performance characteristics entirely.

Why bool[] instead of HashSet<int>? Since our nodes are indexed from 0 to N-1, we can use a simple boolean array. Checking visited[i] is a single CPU instruction—no hash computation, no collision handling, no potential resizing. This alone provides an additional ~1.9x speedup over HashSet.
Why This Isn't Possible in Java, Go, Node.js, or Python

Here's the fundamental limitation of mainstream garbage-collected languages:
Everything Lives on the Heap

    Java: No true structs. "primitives" are values, but custom types are always heap objects. Project Valhalla has been "coming soon" for years.
    Go: Structs exist, but escape analysis often moves them to the heap. Growable stacks mean heap allocation on growth. No way to guarantee stack placement.
    Node.js/JavaScript: All objects are heap-allocated. No value types. V8's JIT can't optimize away the heap.
    Python: Everything is an object. Even integers are heap objects. Default recursion limit is 1,000!
    Ruby: Same story—all objects, all heap, all GC.

These languages have no way to enforce contiguous value-type memory layouts or use integer indices instead of object references. The runtime always assumes GC-managed heap memory with pointer indirection. Go tries to be clever with escape analysis, but complex data structures almost always end up scattered across the heap.

C# is unique—it lets you explicitly choose: use classes for convenience, or use structs when you need deterministic, cache-friendly performance with contiguous memory. No other mainstream GC language gives you this control.
A Word on Our Good Friend: Tail Call Optimization

Before we continue, I'd be remiss not to mention a beloved technique from the functional programming world—tail call optimization (TCO). To understand why it matters (and why we can't rely on it here), we need to peek under the hood at what a function call actually costs.
Anatomy of a Stack Frame

Every time you call a function, the CPU allocates a stack frame—a contiguous block of memory that holds everything needed to execute that function and return to the caller:

┌─────────────────────────────────────┐  ← Stack Pointer (SP)
│  Local Variables                    │     Points to top of stack
│  (your ints, structs, temps)        │
├─────────────────────────────────────┤
│  Saved Registers                    │     CPU state to restore later
├─────────────────────────────────────┤
│  Return Address                     │     Where to jump back (Program Counter)
├─────────────────────────────────────┤
│  Frame Pointer (FP)                 │     Base of THIS frame
│  (points to previous frame's FP)    │     Enables stack unwinding
├─────────────────────────────────────┤
│  Arguments                          │     Parameters passed to function
│  (values OR pointers to heap →)    │     // Object refs point elsewhere!
└─────────────────────────────────────┘  ← Previous Frame's SP

In imperative, object-oriented languages, each frame carries significant overhead:

    Object references — 8-byte pointers to heap-allocated data
    GC roots — The runtime must track which stack slots contain heap references
    Exception handling — Try/catch blocks add unwinding metadata
    Debug info — Line numbers, variable names for stack traces

This is why each recursive call in "standard mode" consumes so much stack space—the language demands rich context for every frame.
Enter Tail Call Optimization

In functional programming languages like Haskell, Erlang, Scheme, and F#, recursion isn't just common—it's the looping mechanism. There are no for or while loops. You express iteration through recursive function calls.

This would be disastrous without tail call optimization. Here's the insight: if a function's last action is to call another function (including itself), there's no reason to keep the current stack frame around. We're done with it! The compiler can reuse the same frame—effectively transforming the recursive call into a goto with updated parameters.

// Tail-recursive factorial (FP style)
let rec factorial n acc =
    if n <= 1 then acc
    else factorial (n - 1) (n * acc)  // ← Tail position!

// Compiler transforms this into (conceptually):
loop:
    if n <= 1 return acc
    acc = n * acc
    n = n - 1
    goto loop  // No new stack frame!

With TCO, a million recursive calls use O(1) stack space—the same single frame, rewritten each iteration. It's beautiful, elegant, and why functional languages can recurse forever.
Why We Can't Rely on TCO Here

Here's the catch: most imperative languages don't guarantee TCO.

    Java: No TCO. The JVM spec doesn't require it.
    Python: Guido explicitly rejected TCO to preserve stack traces.
    JavaScript: ES6 specified TCO, but only Safari implements it. V8 removed it.
    C#: The CLR supports the .tail IL prefix, but the C# compiler rarely emits it. F# does.
    Go: No TCO by design.

Even when TCO is available, DFS doesn't naturally fit the pattern—we need to do work after the recursive call returns (processing neighbors). True tail recursion requires restructuring the algorithm with accumulators, which sacrifices the natural elegance.

So we're stuck with real stack frames. And that's precisely why the size of each frame matters so much—which brings us back to structs, integer indices, and why C# can go where others can't.
C# in "Hardcore Mode"

Yes, this is C#. It looks strange, yet eerily familiar if you've ever written C.

This is what truly differentiates C# from Java and every other high-level garbage-collected language. Beneath the friendly OOP surface, C# has deep roots in C—and when you need it, you can tap into that power. Structs, pointers, stack allocation, manual memory layout. It's all there.

C# gives you a choice that no other mainstream GC language offers:

    Default mode: Easy, productive, class-based OOP like Java
    Hardcore mode: Structs, stack allocation, value-type arrays, Native AOT—matching C++/Rust performance

Same language. Same codebase. No rewrite. Just flip into hardcore mode when you need it.
The Numbers Don't Lie

With structs, integer indices, and bool[] for visited tracking, we achieved over 8x speed improvement (820% faster) and pushed to 30 million stack frames— that's 2x deeper than standard C# and 1,500x deeper than default Java/Python/Node.js.
Why Not Span<T>?

You might think Span<T> would help, but it's actually a fat pointer—it stores both a pointer (8 bytes) and a length (4-8 bytes), making each Span 12-16 bytes on 64-bit systems. Passing Span<bool> instead of bool[] would increase stack frame size, not decrease it. We tried it—it doesn't help with recursive depth. For recursive DFS, plain arrays win.
This is Part 1: Recursive DFS

This article focuses on recursive DFS—keeping the elegant, natural expression of the algorithm while achieving maximum performance. But recursion isn't always the answer. Coming soon: Part 2 will cover Iterative DFS, where the low-level architecture is fundamentally different. We'll explore explicit stack management, avoiding function call overhead entirely, and pushing even further into HPC territory.
The Cloud Computing Angle

Why does this matter beyond academic benchmarks? Cloud costs.

In cloud computing, you pay for compute time. An 8x+ speedup means:

    8x fewer CPU cycles = 8x lower compute costs
    8x faster response times = better user experience
    8x more throughput = handle 8x more requests per instance

When you're processing millions of graph operations daily—recommendation engines, fraud detection, supply chain optimization—these margins are transformative. A startup burning $10K/month on compute could cut that to $1.2K. An enterprise spending $1M could save $875K.

C# is uniquely positioned here: you can develop with the productivity of Java or Python, then flip into HPC mode for production-critical paths. Same codebase, same team, same deployment—just faster and cheaper.
Benchmark Summary
Default Stack (~1MB) - JIT Mode

No custom thread, no stack modifications - pure default behavior
Mode 	Max Depth Before Crash
Standard C# (classes) 	~9,620
HPC C# (structs + bool[]) 	~12,050

Even in JIT mode, HPC achieves ~25% deeper recursion (12,050 vs 9,620). AOT compilation amplifies this advantage further. Raw data (JSON)
Default Stack (~1MB) - AOT Mode

Native AOT compilation - where HPC shines
Mode 	Max Depth Before Crash 	vs JIT
Standard C# (classes) 	~13,880 	1.44x deeper
HPC C# (structs + bool[]) 	~19,430 	2.02x deeper

AOT unlocks HPC's true potential: Native AOT produces smaller, optimized stack frames. HPC AOT achieves 1.4x deeper recursion than Standard AOT (19,430 vs 13,880), and 2x deeper than JIT mode. Raw data (JSON)
~2GB Stack - Maximum Thread Stack
Recursive DFS - Linear Graph (~2GB Stack, bool[] visited)
Depth 	Standard C# 	HPC C# 	Speedup
1,000,000 	258 ms 	110 ms 	2.3x
3,000,000 	877 ms 	127 ms 	6.9x
5,000,000 	1,510 ms 	202 ms 	7.5x
7,500,000 	2,596 ms 	311 ms 	8.3x
9,000,000 	2,848 ms 	379 ms 	7.5x
10,000,000 	3,139 ms 	414 ms 	7.6x
15,000,000 	5,023 ms 	616 ms 	8.2x
20,000,000 	CRASH 	~800 ms 	HPC only
25,000,000 	CRASH 	~1,000 ms 	HPC only
30,000,000 	CRASH 	~1,230 ms 	HPC only

HPC mode: Structs + integer indices + bool[] visited tracking. Standard C# uses class-based nodes with HashSet<Node>. Raw data (JSON)
Why This Matters

Deep graph traversal appears everywhere:

    Compiler implementations — AST traversal, optimization passes
    Game engines — Scene graphs, pathfinding, AI decision trees
    Data processing — Deep JSON/XML parsing, dependency resolution
    AI/ML — Monte Carlo tree search, minimax, neural network graphs

With C# in HPC mode, you don't have to sacrifice the elegance of recursive algorithms for performance. You can have both.
Keywords

DFS depth first search recursion stack overflow structs value types high performance algorithms graph traversal Java Go Node.js Python bool array HashSet optimization
Related Articles

HPC C#
HPC C# (Hardcore Mode)

Did you know C# has a hardcore mode that rivals C++ and Rust performance? Discover the 5 features that make it possible—and why Go can't keep up.
About the Author

Robert Long
Robert Long

Senior Software Engineer

10+ years building cloud-native, high-performance software using first-principles thinking.

© 2026 Robert Long. All rights reserved.

Software Engineer • First Principles Approach

Orange County, California
