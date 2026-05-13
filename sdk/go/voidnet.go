package voidnet

import (
	"errors"
	"strings"
)

const Scheme = "void"

type URI struct {
	Authority string
	Path      string
	Query     string
}

func ParseURI(raw string) (URI, error) {
	const prefix = "void://"
	if !strings.HasPrefix(raw, prefix) {
		return URI{}, errors.New("VOID URI must start with void://")
	}

	rest := strings.TrimPrefix(raw, prefix)
	query := ""
	if head, tail, ok := strings.Cut(rest, "?"); ok {
		rest = head 
		query = tail
	}

	authority := rest
	path := "/"
	if head, tail, ok := strings.Cut(rest, "/"); ok {
		authority = head
		path = "/" + tail
	}

	if authority == "" {
		return URI{}, errors.New("VOID URI authority is missing")
	}

	return URI{
		Authority: authority,
		Path:      path,
		Query:     query,
	}, nil
}

func (uri URI) IsVoidDomain() bool {
	return strings.HasSuffix(uri.Authority, ".void")
}

type PeerID string

type BootstrapNode struct {
	PeerID    PeerID
	Addresses []string
}

