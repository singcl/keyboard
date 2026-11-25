package singleinstance

import "testing"

func TestAcquire(t *testing.T) {
	release, ok := Acquire("keyboard-test-mutex")
	if !ok {
		t.Fatal("first Acquire should succeed")
	}

	if _, ok2 := Acquire("keyboard-test-mutex"); ok2 {
		t.Fatal("second Acquire should fail while first is held")
	}

	release()

	release2, ok3 := Acquire("keyboard-test-mutex")
	if !ok3 {
		t.Fatal("Acquire should succeed after release")
	}
	release2()
}
