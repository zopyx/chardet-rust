"""Tests for scripts/utils.py test-data ref logic."""

from __future__ import annotations

from pathlib import Path
from unittest.mock import patch

import pytest
import utils


class TestGetTestDataRef:
    """Tests for _get_test_data_ref()."""

    def test_release_version(self) -> None:
        with patch("chardet.__version__", "7.0.1"):
            assert utils._get_test_data_ref() == "7.0.1"

    def test_dev_version(self) -> None:
        with patch("chardet.__version__", "7.0.2.dev5+g91df78e"):
            assert utils._get_test_data_ref() is None

    def test_post_version(self) -> None:
        with patch("chardet.__version__", "6.0.0.post1"):
            assert utils._get_test_data_ref() == "6.0.0.post1"

    def test_rc_version(self) -> None:
        with patch("chardet.__version__", "7.0.0rc4"):
            assert utils._get_test_data_ref() == "7.0.0rc4"


class TestGetDataDir:
    """Tests for get_data_dir() cache and clone logic."""

    def _setup_fake_repo(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
        """Point utils.__file__ at a fake repo root so get_data_dir uses tmp_path."""
        fake_scripts = tmp_path / "scripts"
        fake_scripts.mkdir()
        monkeypatch.setattr(utils, "__file__", str(fake_scripts / "utils.py"))

    def test_cache_hit_skips_clone(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        data_dir = tmp_path / "tests" / "data"
        data_dir.mkdir(parents=True)
        (data_dir / utils._REF_FILE).write_text("main\n")
        (data_dir / "utf-8-en").mkdir()

        self._setup_fake_repo(tmp_path, monkeypatch)

        clone_calls: list[str | None] = []

        def fake_clone(local_data: Path, *, ref: str | None) -> None:
            clone_calls.append(ref)

        monkeypatch.setattr(utils, "_clone_test_data", fake_clone)
        monkeypatch.setattr(utils, "_get_test_data_ref", lambda: None)

        result = utils.get_data_dir()
        assert result == data_dir
        assert clone_calls == []

    def test_cache_miss_triggers_reclone(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        data_dir = tmp_path / "tests" / "data"
        data_dir.mkdir(parents=True)
        (data_dir / utils._REF_FILE).write_text("7.0.0\n")
        (data_dir / "utf-8-en").mkdir()

        self._setup_fake_repo(tmp_path, monkeypatch)

        clone_calls: list[tuple[Path, str | None]] = []

        def fake_clone(local_data: Path, *, ref: str | None) -> None:
            clone_calls.append((local_data, ref))

        monkeypatch.setattr(utils, "_clone_test_data", fake_clone)
        monkeypatch.setattr(utils, "_get_test_data_ref", lambda: "7.0.1")

        utils.get_data_dir()

        assert len(clone_calls) == 1
        assert clone_calls[0][1] == "7.0.1"
        # The old directory should have been removed by shutil.rmtree before
        # _clone_test_data was called, so the subdir we created is gone.
        assert not (data_dir / "utf-8-en").exists()

    def test_missing_ref_file_triggers_reclone(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        data_dir = tmp_path / "tests" / "data"
        data_dir.mkdir(parents=True)
        (data_dir / "utf-8-en").mkdir()
        # No _REF_FILE written

        self._setup_fake_repo(tmp_path, monkeypatch)

        clone_calls: list[str | None] = []

        def fake_clone(local_data: Path, *, ref: str | None) -> None:
            clone_calls.append(ref)

        monkeypatch.setattr(utils, "_clone_test_data", fake_clone)
        monkeypatch.setattr(utils, "_get_test_data_ref", lambda: "7.0.1")

        utils.get_data_dir()
        assert len(clone_calls) == 1

    def test_empty_dir_triggers_clone(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        data_dir = tmp_path / "tests" / "data"
        data_dir.mkdir(parents=True)
        # Directory exists but is empty

        self._setup_fake_repo(tmp_path, monkeypatch)

        clone_calls: list[tuple[Path, str | None]] = []

        def fake_clone(local_data: Path, *, ref: str | None) -> None:
            clone_calls.append((local_data, ref))

        monkeypatch.setattr(utils, "_clone_test_data", fake_clone)
        monkeypatch.setattr(utils, "_get_test_data_ref", lambda: None)

        utils.get_data_dir()
        assert len(clone_calls) == 1
        assert clone_calls[0][1] is None

    def test_symlink_used_as_is(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        # Create a real target directory and a symlink to it
        target = tmp_path / "real_data"
        target.mkdir()
        data_dir = tmp_path / "tests" / "data"
        data_dir.parent.mkdir(parents=True)
        data_dir.symlink_to(target)

        self._setup_fake_repo(tmp_path, monkeypatch)

        clone_calls: list[str | None] = []

        def fake_clone(local_data: Path, *, ref: str | None) -> None:
            clone_calls.append(ref)

        monkeypatch.setattr(utils, "_clone_test_data", fake_clone)

        result = utils.get_data_dir()
        assert clone_calls == []
        assert result == target.resolve()
