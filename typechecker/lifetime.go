package typechecker

import "iris-lang/token"

// Region identifies a pipeline scope for lifetime tracking.
// Each nested pipeline (including headless pipelines and fn expression bodies)
// gets a fresh region with a higher depth.
type Region int

// tagBinding records a value snapshot created by :> in a specific region.
type tagBinding struct {
	typ    Type
	region Region
}

// LifetimeTracker tracks :> tag bindings and their lifetimes.
// It is owned by the Checker and threaded through pipeline analysis.
type LifetimeTracker struct {
	tags          map[string]tagBinding // tag name → binding
	currentRegion Region
	nextID        Region
}

func newLifetimeTracker() *LifetimeTracker {
	return &LifetimeTracker{
		tags:   make(map[string]tagBinding),
		nextID: 1, // 0 = top-level / static
	}
}

// enterPipeline creates a new nested region and returns its ID.
func (lt *LifetimeTracker) enterPipeline() Region {
	r := lt.nextID
	lt.nextID++
	lt.currentRegion = r
	return r
}

// exitPipeline restores the region to the parent and invalidates tags that were
// created in the leaving region.
func (lt *LifetimeTracker) exitPipeline(entered Region) {
	for name, b := range lt.tags {
		if b.region == entered {
			delete(lt.tags, name)
		}
	}
	lt.currentRegion = entered - 1 // parent is always lower ID
}

// bindTag records a :> tag in the current region.
func (lt *LifetimeTracker) bindTag(name string, typ Type) {
	lt.tags[name] = tagBinding{typ: typ, region: lt.currentRegion}
}

// lookupTag returns the tag binding if it is alive; second return is false if
// the tag doesn't exist (dead or never created).
func (lt *LifetimeTracker) lookupTag(name string) (tagBinding, bool) {
	b, ok := lt.tags[name]
	return b, ok
}

// checkTagEscape verifies that a tag reference ($name or :& name) is alive.
// Returns a TypeError if the tag is not found.
func (lt *LifetimeTracker) checkTagEscape(name string, tok token.Token) *TypeError {
	if _, ok := lt.tags[name]; !ok {
		return &TypeError{
			Tok:     tok,
			Kind:    ErrLifetimeDeadTag,
			Message: "tag '" + name + "' is not alive in this scope (never created, or escaped its pipeline)",
		}
	}
	return nil
}
