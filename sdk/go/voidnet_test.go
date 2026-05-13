package voidnet

import "testing"

func TestParseURI(t *testing.T) {
	uri, err := ParseURI("void://chat.void/rooms/main?limit=32")
	if err != nil {
		t.Fatal(err)
	}

	if uri.Authority != "chat.void" {
		t.Fatalf("authority = %q", uri.Authority)
	}

	if uri.Path != "/rooms/main" {
		t.Fatalf("path = %q", uri.Path)
	}

	if !uri.IsVoidDomain() {
		t.Fatal("expected .void domain")
	}
}
 
