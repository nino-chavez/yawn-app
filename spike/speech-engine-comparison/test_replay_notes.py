import json
import sys
import tempfile
import unittest
import unittest.mock
import wave
from pathlib import Path

from replay_notes import _bound_native_segments, main, note_segments, sha


class NoteSegmentationTests(unittest.TestCase):
    @staticmethod
    def write_audio(path: Path) -> None:
        with wave.open(str(path), 'wb') as output:
            output.setnchannels(1)
            output.setsampwidth(2)
            output.setframerate(16_000)
            output.writeframes(b'\0\0' * 16_000)

    def write_native_input(
        self, root: Path, engine: str, *, malformed: bool = False,
    ) -> tuple[Path, Path, Path]:
        audio = root / 'audio.wav'
        self.write_audio(audio)
        words = [
            {'text': 'word', 'start': index / 100, 'end': index / 100 + 0.005}
            for index in range(41)
        ]
        text = ' '.join(item['text'] for item in words)
        run = {
            'text': text,
            'timing_issue_count': False if malformed else 0,
            'segments': words,
            'utterances': (
                [{'text': text, 'start': 0, 'end': words[-1]['end']}]
                if engine == 'apple' else []
            ),
        }
        result = root / f'input.{engine}.json'
        result.write_text(json.dumps({
            'schema': 'speech-engine-result/1', 'status': 'ok', 'engine': engine,
            'runs': [run],
        }))
        receipt = root / f'input.{engine}.receipt.json'
        receipt.write_text(json.dumps({
            'schema': 'speech-engine-attempt/1', 'status': 'ok',
            'source_unchanged': True, 'engine': engine,
            'result_sha256': sha(result), 'audio_sha256': sha(audio),
        }))
        return audio, result, receipt

    def test_native_utterances_are_preserved(self):
        utterances = [{'text': 'Two words.', 'start': 1, 'end': 2}]
        result = {'engine': 'apple', 'runs': [{'utterances': utterances, 'segments': []}]}
        self.assertIs(note_segments(result)[0], utterances)

    def test_word_grouping_preserves_words_and_native_bounds(self):
        words = [
            {'text': 'Keep', 'start': 1, 'end': 1.2},
            {'text': 'this.', 'start': 1.3, 'end': 1.6},
            {'text': 'Then', 'start': 2, 'end': 2.3},
            {'text': 'pause', 'start': 4, 'end': 4.5},
            {'text': 'long', 'start': 19, 'end': 20},
        ]
        result = {'engine': 'parakeet', 'runs': [{'segments': words}]}
        groups, _ = note_segments(result)
        self.assertEqual(' '.join(g['text'] for g in groups), ' '.join(w['text'] for w in words))
        self.assertEqual([(g['start'], g['end']) for g in groups], [(1, 1.6), (2, 2.3), (4, 4.5), (19, 20)])

    def test_whisper_segments_are_not_regrouped(self):
        segments = [{'text': 'Keep source grouping', 'start': 1, 'end': 7}]
        self.assertIs(note_segments({'engine': 'whisper', 'runs': [{'segments': segments}]})[0], segments)

    def test_native_result_refuses_an_unbound_or_wrong_audio_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            audio = root / 'mic.wav'
            with wave.open(str(audio), 'wb') as output:
                output.setnchannels(1)
                output.setsampwidth(2)
                output.setframerate(16_000)
                output.writeframes(b'\0\0' * 16_000)
            result = root / 'mic.parakeet.json'
            result.write_text(json.dumps({
                'schema': 'speech-engine-result/1', 'status': 'ok', 'engine': 'parakeet',
                'runs': [{'text': 'word', 'timing_issue_count': 0,
                          'segments': [{'text': 'word', 'start': 0, 'end': 0.2}],
                          'utterances': []}],
            }))
            receipt = root / 'mic.parakeet.receipt.json'
            receipt.write_text(json.dumps({
                'schema': 'speech-engine-attempt/1', 'status': 'ok',
                'source_unchanged': True, 'engine': 'parakeet',
                'result_sha256': sha(result), 'audio_sha256': sha(audio),
            }))
            self.assertEqual(
                _bound_native_segments(result, receipt, audio, engine='parakeet')[0],
                [{'text': 'word', 'start': 0.0, 'end': 0.2}],
            )
            receipt.write_text(json.dumps({**json.loads(receipt.read_text()), 'audio_sha256': '0' * 64}))
            with self.assertRaisesRegex(ValueError, 'audio bytes'):
                _bound_native_segments(result, receipt, audio, engine='parakeet')

    def test_bounded_grouping_uses_raw_validated_words_for_both_native_engines(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for engine in ('apple', 'parakeet'):
                with self.subTest(engine=engine):
                    audio, result, receipt = self.write_native_input(root, engine)
                    current, _current_provenance, current_exceptions = _bound_native_segments(
                        result, receipt, audio, engine=engine,
                    )
                    self.assertEqual(len(current), 1)
                    self.assertEqual(current_exceptions, [])
                    turns, provenance, exceptions = _bound_native_segments(
                        result, receipt, audio, engine=engine, grouping='bounded',
                    )
                    self.assertEqual(len(turns), 2)
                    self.assertEqual(
                        [(turn['start'], turn['end']) for turn in turns],
                        [(0.0, 0.395), (0.4, 0.405)],
                    )
                    self.assertIn('40 lexical tokens', provenance)
                    self.assertEqual(exceptions, [])

    def test_native_refusals_happen_before_replay_output_exists(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            capture = root / 'capture'
            capture.mkdir()
            for name in ('mic.wav', 'system.wav'):
                self.write_audio(capture / name)
            (capture / 'session.json').write_text('{}')
            mic_audio, mic_result, mic_receipt = self.write_native_input(
                root, 'parakeet', malformed=True,
            )
            system_result = root / 'system.parakeet.json'
            system_result.write_bytes(mic_result.read_bytes())
            system_receipt = root / 'system.parakeet.receipt.json'
            system_receipt.write_text(json.dumps({
                **json.loads(mic_receipt.read_text()),
                'result_sha256': sha(system_result),
                'audio_sha256': sha(capture / 'system.wav'),
            }))
            mic_receipt.write_text(json.dumps({
                **json.loads(mic_receipt.read_text()),
                'audio_sha256': sha(capture / 'mic.wav'),
            }))
            out = root / 'out'
            arguments = [
                'replay_notes.py', '--capture', str(capture), '--engine', 'parakeet',
                '--mic-result', str(mic_result), '--system-result', str(system_result),
                '--mic-receipt', str(mic_receipt), '--system-receipt', str(system_receipt),
                '--note-model', str(root / 'missing-model'), '--catalog', str(root / 'catalog'),
                '--generate-site-packages', str(root / 'site-packages'), '--out', str(out),
                '--transcript-only',
            ]
            with (
                unittest.mock.patch('verify_capture.verify_acquisition'),
                unittest.mock.patch.object(sys, 'argv', arguments),
                self.assertRaisesRegex(Exception, 'timing'),
            ):
                main()
            self.assertFalse(out.exists())

    def test_wrong_native_source_and_bounded_whisper_refuse_before_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            capture = root / 'capture'
            capture.mkdir()
            for name in ('mic.wav', 'system.wav'):
                self.write_audio(capture / name)
            (capture / 'session.json').write_text('{}')
            _audio, result, receipt = self.write_native_input(root, 'parakeet')
            receipt.write_text(json.dumps({
                **json.loads(receipt.read_text()),
                'audio_sha256': '0' * 64,
            }))
            out = root / 'out'
            arguments = [
                'replay_notes.py', '--capture', str(capture), '--engine', 'parakeet',
                '--mic-result', str(result), '--system-result', str(result),
                '--mic-receipt', str(receipt), '--system-receipt', str(receipt),
                '--note-model', str(root / 'missing-model'), '--catalog', str(root / 'catalog'),
                '--generate-site-packages', str(root / 'site-packages'), '--out', str(out),
                '--transcript-only',
            ]
            with (
                unittest.mock.patch('verify_capture.verify_acquisition'),
                unittest.mock.patch.object(sys, 'argv', arguments),
                self.assertRaisesRegex(ValueError, 'audio bytes'),
            ):
                main()
            self.assertFalse(out.exists())

            whisper_out = root / 'whisper-out'
            whisper_arguments = [
                'replay_notes.py', '--capture', str(capture), '--engine', 'whisper',
                '--grouping', 'bounded', '--mic-result', str(result),
                '--system-result', str(result), '--note-model', str(root / 'missing-model'),
                '--catalog', str(root / 'catalog'), '--generate-site-packages', str(root / 'site-packages'),
                '--out', str(whisper_out), '--transcript-only',
            ]
            with (
                unittest.mock.patch.object(sys, 'argv', whisper_arguments),
                self.assertRaises(SystemExit),
            ):
                main()
            self.assertFalse(whisper_out.exists())


if __name__ == '__main__':
    unittest.main()
