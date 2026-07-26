import 'package:flutter/material.dart';

/// A vector rendering of the desktop App Tester mark, kept code-native so the
/// Android app and desktop app share the same recognizable silhouette.
class AppTesterMark extends StatelessWidget {
  const AppTesterMark({this.size = 64, super.key});

  final double size;

  @override
  Widget build(BuildContext context) => SizedBox.square(
        dimension: size,
        child: CustomPaint(painter: _AppTesterMarkPainter()),
      );
}

class _AppTesterMarkPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    final unit = size.width / 128;
    final bounds = Rect.fromLTWH(0, 0, size.width, size.height);
    final panel = Paint()..color = const Color(0xff071632);
    canvas.drawRRect(RRect.fromRectAndRadius(bounds, Radius.circular(28 * unit)), panel);

    final line = Paint()
      ..color = const Color(0xffdce5f2)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 3 * unit
      ..strokeCap = StrokeCap.round;
    final accent = Paint()
      ..color = const Color(0xff2be0a7)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 3 * unit
      ..strokeCap = StrokeCap.round;
    final phone = RRect.fromRectAndRadius(
      Rect.fromLTWH(46 * unit, 24 * unit, 36 * unit, 80 * unit),
      Radius.circular(7 * unit),
    );
    canvas.drawRRect(phone, line);
    canvas.drawLine(Offset(58 * unit, 31 * unit), Offset(70 * unit, 31 * unit), line);
    canvas.drawLine(Offset(59 * unit, 96 * unit), Offset(69 * unit, 96 * unit), line);
    for (final path in [
      [Offset(38 * unit, 42 * unit), Offset(34 * unit, 42 * unit), Offset(34 * unit, 59 * unit)],
      [Offset(38 * unit, 85 * unit), Offset(34 * unit, 85 * unit), Offset(34 * unit, 69 * unit)],
      [Offset(90 * unit, 42 * unit), Offset(95 * unit, 42 * unit), Offset(95 * unit, 59 * unit)],
      [Offset(90 * unit, 85 * unit), Offset(95 * unit, 85 * unit), Offset(95 * unit, 69 * unit)],
    ]) {
      canvas.drawLine(path[0], path[1], accent);
      canvas.drawLine(path[1], path[2], accent);
    }
    canvas.drawCircle(Offset(64 * unit, 64 * unit), 14 * unit, accent);
    canvas.drawLine(Offset(57 * unit, 64 * unit), Offset(62 * unit, 69 * unit), accent);
    canvas.drawLine(Offset(62 * unit, 69 * unit), Offset(72 * unit, 57 * unit), accent);
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => false;
}
