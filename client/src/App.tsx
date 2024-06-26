import { useEffect, useRef } from 'react';
import './App.css';

const App = () => {
  const videoRef = useRef<HTMLCanvasElement | null>(null);
  const ws = new WebSocket(import.meta.env.VITE_API_ROOT as string);

  useEffect(() => {
    ws.onopen = () => {
      console.log('WebSocket is connected.');
    };

    ws.onmessage = event => {
      // 수신한 메시지가 바이너리 데이터일 경우 (이미지 데이터)
      const blob = new Blob([event.data], { type: 'image/jpeg' });
      const url = URL.createObjectURL(blob);
      const img = new Image();
      img.src = url;
      img.onload = () => {
        if (videoRef.current) {
          const context = videoRef.current.getContext('2d');

          if (context) {
            context.drawImage(img, 0, 0, videoRef.current.width, videoRef.current.height);
            URL.revokeObjectURL(url);
          }
        }
      };
    };

    ws.onclose = () => {
      console.log('WebSocket is closed.');
    };

    ws.onerror = error => {
      console.error('WebSocket error:', error);
    };

    return () => {
      if (ws.readyState === 1) {
        ws.close();
      }
    };
  }, [ws]);

  return (
    <>
      <div className="card">
        <canvas className="webcam" ref={videoRef} />
        <p>Live face detection using WebSockets and OpenCV</p>
      </div>
      <a className="read-the-docs" href="https://github.com/Yummy-sk/live-stream">
        Go to repo
      </a>
    </>
  );
};

export default App;
